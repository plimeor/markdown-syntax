//! Markdown source to AST. The entry points are the free [`parse`] function
//! (maximal default dialect) and the [`SyntaxOptions::parse`] /
//! [`SyntaxOptions::parse_strict`] methods. Parsing is tolerant: problems are
//! collected as [`Diagnostic`]s rather than aborting.

use alloc::{borrow::Cow, collections::BTreeMap, string::String, vec, vec::Vec};

use crate::{
    ast::*,
    diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity},
    entities::named_character_reference,
    memo::{
        bracket_walk, path_walk, pattern_starts, BracketMemo, BracketStep, PathMemo, PersistentMap,
        Positions, Step,
    },
    options::{SyntaxConfigError, SyntaxOptions},
    span::Span,
    validate::is_directive_name,
};

mod blocks;
mod nul_replacement;
#[cfg(test)]
mod scan_tests;
mod source_map;

use source_map::{DerivedText, Segment, SourceMap};

/// The result of a tolerant parse: the document plus any diagnostics gathered
/// along the way (empty on a clean parse).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseOutput {
    /// The parsed document tree.
    pub document: Document,
    /// Diagnostics collected during parsing.
    pub diagnostics: Vec<Diagnostic>,
}

/// The error returned by [`SyntaxOptions::parse_strict`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseStrictError {
    /// The options themselves were contradictory.
    Config(SyntaxConfigError),
    /// An error-severity diagnostic was promoted to a hard failure.
    Diagnostic(Diagnostic),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedLinkResource {
    destination: String,
    destination_kind: LinkDestinationKind,
    title: Option<String>,
    title_kind: Option<LinkTitleKind>,
}

const REFERENCE_LABEL_MAX_CHARS: usize = 999;
const WIKILINK_MAX_BYTES: usize = 999;

/// One line of the text a block parser reads. `start`, `end`, and
/// `end_with_eol` are positions in the original input; offsets into `text`
/// translate to the input through `source_start` / `source_end`, since a
/// container's lines are derived from its source lines with prefixes removed.
#[derive(Clone, Copy, Debug)]
struct Line<'a> {
    text: &'a str,
    eol: &'a str,
    start: usize,
    end: usize,
    end_with_eol: usize,
    /// The source-map segments covering `text` and `eol`, in the coordinates of
    /// the string the line was split from, where `text` starts at `text_offset`.
    segments: &'a [Segment],
    text_offset: usize,
    /// The source column `text` starts at, from which its tabs reach their
    /// tab stops (every four columns).
    column: usize,
}

impl<'a> Line<'a> {
    /// A line checked for its shape alone, whose positions are not used.
    fn detached(text: &'a str) -> Self {
        Line {
            text,
            eol: "",
            start: 0,
            end: text.len(),
            end_with_eol: text.len(),
            segments: &[],
            text_offset: 0,
            column: 0,
        }
    }

    /// The source column after `self.text[..offset]`.
    fn column_at(&self, offset: usize) -> usize {
        advance_columns(self.column, &self.text[..offset])
    }

    /// The input position where a node starting at byte `offset` of `text`
    /// starts.
    fn source_start(&self, offset: usize) -> usize {
        source_map::start_of(self.segments, self.text_offset + offset)
    }

    /// The input position where a node ending at byte `offset` of `text` ends.
    fn source_end(&self, offset: usize) -> usize {
        source_map::end_of(self.segments, self.text_offset + offset)
    }

    /// The input range of this line's ending.
    fn eol_source(&self) -> (usize, usize) {
        (self.end, self.end_with_eol)
    }

    /// Records in `map`, from text position `at`, what `text[from..to]` was
    /// read from.
    fn copy_into(&self, map: &mut SourceMap, at: usize, from: usize, to: usize) {
        map.copy(
            at,
            self.segments,
            self.text_offset + from,
            self.text_offset + to,
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HtmlBlockKind {
    RawTag,
    BlockTag,
    Until(&'static str),
    UntilBlank,
}

/// Parse `input` under the maximal default dialect ([`SyntaxOptions::default`]).
/// Infallible and tolerant; sugar for `SyntaxOptions::default().parse(input)`.
pub fn parse(input: &str) -> ParseOutput {
    SyntaxOptions::default().parse(input)
}

impl SyntaxOptions {
    /// Parse `input` under these options. Infallible and tolerant: a config
    /// conflict (reachable only by hand-building contradictory `Constructs`) is
    /// surfaced as an error diagnostic rather than a hard error. Call
    /// [`SyntaxOptions::validate`] first for fail-fast config checking.
    pub fn parse(&self, input: &str) -> ParseOutput {
        match parse_checked(input, self) {
            Ok(output) => output,
            Err(error) => ParseOutput {
                document: Document::default(),
                diagnostics: vec![Diagnostic::new(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::StrictParse,
                    Span::new(0, input.len()),
                    error.message(),
                )],
            },
        }
    }

    /// Parse `input`, promoting a config conflict or any error-severity
    /// diagnostic to a hard [`ParseStrictError`].
    pub fn parse_strict(&self, input: &str) -> Result<ParseOutput, ParseStrictError> {
        let output = parse_checked(input, self).map_err(ParseStrictError::Config)?;
        if let Some(diagnostic) = output
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
        {
            return Err(ParseStrictError::Diagnostic(diagnostic.clone()));
        }
        Ok(output)
    }
}

fn parse_checked(input: &str, options: &SyntaxOptions) -> Result<ParseOutput, SyntaxConfigError> {
    options.validate()?;
    Ok(parse_with_definitions(input, options, &[]))
}

/// Parses `input` as if it also held a definition of each label in `known`,
/// sorted and deduplicated identifiers: the serializer reads its own output
/// back this way, as the document it was written from resolves its
/// references. Options are not validated.
/// Parses `input` with each identifier in `known`, sorted and deduplicated,
/// read as defined.
pub(crate) fn parse_with_definitions(
    input: &str,
    options: &SyntaxOptions,
    known: &[String],
) -> ParseOutput {
    let mut diagnostics = Vec::new();
    // A leading byte order mark is not content; parsing starts after it while
    // spans keep counting from the start of `input`.
    let start = if input.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let source = &input[start..];
    let map = SourceMap::verbatim(source.len(), start);
    let lines = collect_lines(source, &map);
    let children = blocks::parse_document(&lines, options, known, &mut diagnostics);
    let mut document = Document {
        meta: NodeMeta::new(Some(Span::new(0, input.len()))),
        children,
    };
    if source.contains('\0') {
        nul_replacement::replace_in_document(&mut document);
    }

    ParseOutput {
        document,
        diagnostics,
    }
}

/// CommonMark reads U+0000 as U+FFFD. Character classifications that differ
/// between the two call this; node values are rewritten after parsing by
/// `nul_replacement::replace_in_document`, so spans stay in source coordinates.
fn source_char(char: char) -> char {
    if char == '\0' {
        '\u{FFFD}'
    } else {
        char
    }
}

/// The deepest block-container nesting (block quotes, list items, container
/// directives, footnote definitions, HTML containers, description details) the
/// parser opens. Deeper container markers stay leaf-block text, so recursion
/// and the native stack it uses stay bounded.
const MAX_BLOCK_NESTING: usize = 32;

/// Parses the blocks of a container's derived content.
/// The column `text` reaches from column `column`, with tab stops every four.
fn advance_columns(column: usize, text: &str) -> usize {
    text.chars().fold(column, |column, char| {
        if char == '\t' {
            column + 4 - column % 4
        } else {
            column + 1
        }
    })
}

/// Splits `input` into lines, with positions translated through `map`, the
/// source map of `input`.
fn collect_lines<'a>(input: &'a str, map: &'a SourceMap) -> Vec<Line<'a>> {
    let bytes = input.as_bytes();
    let mut lines = Vec::new();
    let mut segments = LineSegments {
        segments: map.segments(),
        first: 0,
    };
    let mut start = 0;
    let mut index = 0;

    while index < bytes.len() {
        let eol_end = match bytes[index] {
            b'\n' => index + 1,
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => index + 2,
            b'\r' => index + 1,
            _ => {
                index += 1;
                continue;
            }
        };
        lines.push(segments.line(input, start, index, eol_end));
        index = eol_end;
        start = index;
    }

    if start < bytes.len() || input.is_empty() {
        lines.push(segments.line(input, start, bytes.len(), bytes.len()));
    }

    lines
}

/// Hands each line of a split string the segments that cover it, moving
/// forward through the map once.
struct LineSegments<'a> {
    segments: &'a [Segment],
    first: usize,
}

impl<'a> LineSegments<'a> {
    fn line(&mut self, input: &'a str, start: usize, end: usize, eol_end: usize) -> Line<'a> {
        let segments = self.segments;
        while self.first + 1 < segments.len() && segments[self.first].text_end() <= start {
            self.first += 1;
        }
        let mut last = self.first;
        while last + 1 < segments.len() && segments[last + 1].text_start() < eol_end {
            last += 1;
        }
        let covering = if segments.is_empty() {
            segments
        } else {
            &segments[self.first..=last]
        };
        let source_start = source_map::start_of(covering, start);
        let source_end = if end == start {
            source_start
        } else {
            source_map::end_of(covering, end)
        };
        let source_end_with_eol = if eol_end == end {
            source_end
        } else {
            source_map::end_of(covering, eol_end)
        };
        Line {
            text: &input[start..end],
            eol: &input[end..eol_end],
            start: source_start,
            end: source_end,
            end_with_eol: source_end_with_eol,
            segments: covering,
            text_offset: start,
            column: 0,
        }
    }
}

fn frontmatter_fence_kind(line: &str) -> Option<FrontmatterKind> {
    match line.trim_end_matches([' ', '\t']) {
        "---" => Some(FrontmatterKind::Yaml),
        "+++" => Some(FrontmatterKind::Toml),
        _ => None,
    }
}

fn directive_container_opener_prefix(input: &str) -> Option<(usize, &str)> {
    let fence_len = input
        .as_bytes()
        .iter()
        .take_while(|byte| **byte == b':')
        .count();
    if fence_len >= 3 {
        Some((fence_len, &input[fence_len..]))
    } else {
        None
    }
}

fn directive_container_closing_fence(input: &str, min_len: usize) -> Option<usize> {
    let fence_len = input
        .as_bytes()
        .iter()
        .take_while(|byte| **byte == b':')
        .count();
    if fence_len >= min_len && is_blank(&input[fence_len..]) {
        Some(fence_len)
    } else {
        None
    }
}

/// Length of the leading `$` run if `input` (already indent-stripped) is a valid
/// math-flow opener: `>=2` dollars, then an info string with no further `$`.
fn math_block_fence_length(input: &str) -> Option<usize> {
    let length = input
        .as_bytes()
        .iter()
        .take_while(|byte| **byte == b'$')
        .count();
    if length < 2 || input[length..].contains('$') {
        return None;
    }
    Some(length)
}

/// A math-flow closing line (already indent-stripped) is a run of `>=length`
/// dollars and nothing else (trailing whitespace aside).
fn math_block_fence_closes(input: &str, length: usize) -> bool {
    let count = input
        .as_bytes()
        .iter()
        .take_while(|byte| **byte == b'$')
        .count();
    count >= length && is_blank(&input[count..])
}

fn parse_alert_marker(line: &str) -> Option<(AlertKind, Option<String>)> {
    let close = line.find(']')?;
    let marker = line.get(0..close + 1)?;
    if !marker.starts_with("[!") {
        return None;
    }
    let kind = match &marker[2..close].to_ascii_lowercase()[..] {
        "note" => AlertKind::Note,
        "tip" => AlertKind::Tip,
        "important" => AlertKind::Important,
        "warning" => AlertKind::Warning,
        "caution" => AlertKind::Caution,
        _ => return None,
    };
    let title = line[close + 1..].trim_matches([' ', '\t']);
    Some((
        kind,
        if title.is_empty() {
            None
        } else {
            Some(title.into())
        },
    ))
}

/// Whether `text` starts with a description details marker.
fn is_description_marker(text: &str) -> bool {
    matches!(text.as_bytes().first(), Some(b':' | b'~'))
        && matches!(text.as_bytes().get(1), None | Some(b' ' | b'\t'))
}

fn parse_thematic_break(line: Line<'_>) -> Option<Block> {
    let text = trim_up_to_three_spaces(line.text)?;
    let mut marker = None;
    let mut count = 0;
    for char in text.chars() {
        if char == ' ' || char == '\t' {
            continue;
        }
        let current = match char {
            '-' => ThematicBreakMarker::Dash,
            '*' => ThematicBreakMarker::Asterisk,
            '_' => ThematicBreakMarker::Underscore,
            _ => return None,
        };
        if marker.is_some_and(|marker| marker != current) {
            return None;
        }
        marker = Some(current);
        count += 1;
    }
    if count >= 3 {
        Some(Block::ThematicBreak(ThematicBreak {
            meta: NodeMeta::new(Some(Span::new(line.start, line.end))),
            marker: marker?,
        }))
    } else {
        None
    }
}

/// The link reference definition that starts paragraph line `index`, and
/// the line after it. The lines are a paragraph's, each from where the
/// containers around it leave it; the definition's span is left to the
/// caller.
fn parse_definition(lines: &[Line<'_>], index: usize) -> Option<(Definition, usize)> {
    let text = trim_ascii_start(lines[index].text);
    if !text.starts_with('[') {
        return None;
    }

    // A label may span lines (CommonMark §4.7), up to its length limit.
    let mut accumulated = String::from(text);
    let mut label_end_line = index;
    let close = loop {
        if let Some(close) = find_reference_label_end(&accumulated, 0) {
            if accumulated.as_bytes().get(close + 1) == Some(&b':') {
                break close;
            }
            // A closed label not followed by `:` is not a definition.
            return None;
        }
        let next = label_end_line + 1;
        if next >= lines.len() || accumulated.len() > 4 * REFERENCE_LABEL_MAX_CHARS + 2 {
            return None;
        }
        accumulated.push('\n');
        accumulated.push_str(lines[next].text);
        label_end_line = next;
    };
    let label = String::from(&accumulated[1..close]);
    if normalize_label(&label).is_empty() {
        return None;
    }
    let mut source = String::from(&accumulated[close + 2..]);
    let mut cursor = label_end_line;
    let mut best_without_title = None;
    // The char that closes a title the source leaves open: a line without
    // it cannot change the parse, so the source is parsed again only once a
    // line holds it, which keeps a long open title linear.
    let mut open_title: Option<char> = None;

    loop {
        let parses =
            open_title.is_none_or(|closer| line_may_close_title(lines[cursor].text, closer));
        if parses {
            match parse_definition_destination_title(&source) {
                Some(resource) => {
                    if resource.title.is_some() {
                        return Some((definition_node(&label, resource), cursor + 1));
                    }
                    best_without_title = Some((resource, cursor + 1));
                    let next = cursor + 1;
                    if next >= lines.len() || !line_can_start_definition_title(lines[next].text) {
                        break;
                    }
                }
                // A destination, or a title once it has closed, that does
                // not parse cannot parse with more lines.
                None if !is_blank(&source) => match open_definition_title(&source) {
                    Some(closer) if open_title.is_none() => open_title = Some(closer),
                    _ => break,
                },
                None => {}
            }
        }
        let next = cursor + 1;
        if next >= lines.len() {
            break;
        }
        source.push('\n');
        source.push_str(lines[next].text);
        cursor = next;
    }

    let (resource, next) = best_without_title?;
    Some((definition_node(&label, resource), next))
}

fn definition_node(label: &str, resource: ParsedLinkResource) -> Definition {
    Definition {
        meta: NodeMeta::default(),
        label: label.into(),
        identifier: normalize_label(label),
        destination: resource.destination,
        destination_kind: resource.destination_kind,
        title: resource.title,
        title_kind: resource.title_kind,
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum HtmlContainerTag {
    Opening,
    Closing,
}

#[derive(Clone, Copy)]
struct HtmlContainerFence {
    marker: FenceMarker,
    length: usize,
}

/// One step of the line walk to an HTML container's closing tag line: a code
/// fence is stepped over whole, and same-name opening and closing tag lines
/// nest.
fn html_container_close_step(lines: &[Line<'_>], cursor: usize, tag: &str) -> BracketStep {
    let Some(line) = lines.get(cursor) else {
        return BracketStep::End;
    };
    if let Some(fence) = html_container_fence_opens(line) {
        let after_fence = (cursor + 1..lines.len())
            .find(|close| html_container_fence_closes(&lines[*close], fence))
            .map_or(lines.len(), |close| close + 1);
        return BracketStep::Pass(after_fence);
    }
    if parse_html_container_tag_line(*line, tag, HtmlContainerTag::Closing).is_some() {
        return BracketStep::Close(cursor + 1);
    }
    if parse_html_container_opening_line(*line, tag).is_some() {
        return BracketStep::Open(cursor + 1);
    }
    BracketStep::Pass(cursor + 1)
}

fn parse_html_container_tag_line(
    line: Line<'_>,
    tag: &str,
    kind: HtmlContainerTag,
) -> Option<HtmlTag> {
    let (trimmed, indent_bytes) = trim_html_container_line(&line)?;
    let (end, name) = parse_html_tag(trimmed, 0)?;
    if !name.eq_ignore_ascii_case(tag) || !is_blank(&trimmed[end..]) {
        return None;
    }

    let closing = html_tag_is_closing(trimmed, 0);
    if (kind == HtmlContainerTag::Opening && closing)
        || (kind == HtmlContainerTag::Closing && !closing)
        || html_tag_is_self_closing(&trimmed[..end])
    {
        return None;
    }

    Some(HtmlTag {
        meta: NodeMeta::new(Some(Span::new(
            line.source_start(indent_bytes),
            line.source_end(indent_bytes + end),
        ))),
        name: tag.into(),
        raw: trimmed[..end].into(),
    })
}

fn parse_html_container_opening_line(line: Line<'_>, tag: &str) -> Option<HtmlTag> {
    let (trimmed, indent_bytes) = trim_html_container_line(&line)?;
    let (end, name) = parse_html_tag(trimmed, 0)?;
    if !name.eq_ignore_ascii_case(tag)
        || html_tag_is_closing(trimmed, 0)
        || html_tag_is_self_closing(&trimmed[..end])
    {
        return None;
    }

    let rest_start = end + leading_ascii_whitespace_len(&trimmed[end..]);
    if !trimmed[rest_start..].is_empty() && !html_container_line_has_summary(&trimmed[rest_start..])
    {
        return None;
    }

    Some(HtmlTag {
        meta: NodeMeta::new(Some(Span::new(
            line.source_start(indent_bytes),
            line.source_end(indent_bytes + end),
        ))),
        name: tag.into(),
        raw: trimmed[..end].into(),
    })
}

fn html_container_line_has_summary(input: &str) -> bool {
    let Some((open_end, name)) = parse_html_tag(input, 0) else {
        return false;
    };
    if !name.eq_ignore_ascii_case("summary")
        || html_tag_is_closing(input, 0)
        || html_tag_is_self_closing(&input[..open_end])
    {
        return false;
    }
    let Some(close_start) = input[open_end..].find("</").map(|offset| open_end + offset) else {
        return false;
    };
    let Some((close_end, close_name)) = parse_html_tag(input, close_start) else {
        return false;
    };
    close_name.eq_ignore_ascii_case("summary")
        && html_tag_is_closing(input, close_start)
        && is_blank(&input[close_end..])
}

/// `line` from its first char other than a space or tab, when its
/// indentation, from the column it starts at, is at most three columns.
fn trim_html_container_line<'a>(line: &Line<'a>) -> Option<(&'a str, usize)> {
    let (columns, bytes) = leading_indent_at(line.text, line.column);
    (columns <= 3).then(|| (&line.text[bytes..], bytes))
}

fn leading_ascii_whitespace_len(input: &str) -> usize {
    input
        .as_bytes()
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count()
}

fn html_tag_is_closing(input: &str, index: usize) -> bool {
    input.as_bytes().get(index + 1) == Some(&b'/')
}

fn html_tag_is_self_closing(input: &str) -> bool {
    input.trim_end_matches([' ', '\t']).ends_with("/>")
}

fn html_container_fence_opens(line: &Line<'_>) -> Option<HtmlContainerFence> {
    let (trimmed, _) = trim_html_container_line(line)?;
    let (marker, length) = fence_start(trimmed)?;
    Some(HtmlContainerFence { marker, length })
}

fn html_container_fence_closes(line: &Line<'_>, fence: HtmlContainerFence) -> bool {
    trim_html_container_line(line)
        .is_some_and(|(trimmed, _)| fence_close(trimmed, fence.marker, fence.length))
}

fn html_block_start(input: &str) -> Option<HtmlBlockKind> {
    let trimmed = input.trim_end_matches([' ', '\t']);
    if !trimmed.starts_with('<') {
        return None;
    }

    if raw_html_tag_start(trimmed) {
        return Some(HtmlBlockKind::RawTag);
    }
    if trimmed.starts_with("<!--") {
        return Some(HtmlBlockKind::Until("-->"));
    }
    if trimmed.starts_with("<?") {
        return Some(HtmlBlockKind::Until("?>"));
    }
    if is_declaration_start(trimmed) {
        return Some(HtmlBlockKind::Until(">"));
    }
    if trimmed.starts_with("<![CDATA[") {
        return Some(HtmlBlockKind::Until("]]>"));
    }

    if html_block_tag_start(trimmed) {
        return Some(HtmlBlockKind::BlockTag);
    }

    let Some((end, _tag_name)) = parse_html_tag(trimmed, 0) else {
        return None;
    };
    if is_blank(&trimmed[end..]) {
        Some(HtmlBlockKind::UntilBlank)
    } else {
        None
    }
}

fn raw_html_tag_start(input: &str) -> bool {
    for tag in ["script", "pre", "style", "textarea"] {
        if html_raw_open_tag_prefix(input, tag) {
            return true;
        }
    }
    false
}

fn html_raw_open_tag_prefix(input: &str, tag: &str) -> bool {
    let Some(rest) = input.strip_prefix('<') else {
        return false;
    };
    if rest.starts_with('/') || rest.len() < tag.len() {
        return false;
    }
    let rest_bytes = rest.as_bytes();
    let tag_bytes = tag.as_bytes();
    if !rest_bytes
        .get(..tag_bytes.len())
        .is_some_and(|name| name.eq_ignore_ascii_case(tag_bytes))
    {
        return false;
    }
    match rest_bytes.get(tag.len()) {
        None => true,
        Some(b' ' | b'\t' | b'\n' | b'\r' | b'>') => true,
        Some(b'/') => {
            rest_bytes.get(tag.len() + 1) == Some(&b'>') && rest_bytes.get(tag.len() + 2).is_none()
        }
        _ => false,
    }
}

fn line_contains_raw_closing_tag(input: &str, tag: &str) -> bool {
    let bytes = input.as_bytes();
    let tag_bytes = tag.as_bytes();
    let mut cursor = 0;

    while cursor + 2 + tag_bytes.len() <= bytes.len() {
        let tag_start = cursor + 2;
        let tag_end = tag_start + tag_bytes.len();
        if bytes.get(cursor) == Some(&b'<')
            && bytes.get(cursor + 1) == Some(&b'/')
            && bytes
                .get(tag_start..tag_end)
                .is_some_and(|name| name.eq_ignore_ascii_case(tag_bytes))
        {
            match bytes.get(tag_end) {
                Some(b'>') => return true,
                Some(byte) if byte.is_ascii_whitespace() => {
                    let mut after_space = tag_end;
                    while bytes
                        .get(after_space)
                        .is_some_and(|byte| byte.is_ascii_whitespace())
                    {
                        after_space += 1;
                    }
                    if bytes.get(after_space) == Some(&b'>') {
                        return true;
                    }
                }
                _ => {}
            }
        }
        cursor += 1;
    }

    false
}

fn html_block_tag_start(input: &str) -> bool {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'<') {
        return false;
    }

    let mut cursor = 1;
    if bytes.get(cursor) == Some(&b'/') {
        cursor += 1;
    }

    let name_start = cursor;
    if !bytes
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_alphabetic())
    {
        return false;
    }
    cursor += 1;
    while bytes.get(cursor).is_some_and(|byte| html_name_byte(*byte)) {
        cursor += 1;
    }

    let name = &input[name_start..cursor];
    if !html_block_tag(name) {
        return false;
    }

    match bytes.get(cursor) {
        None | Some(b' ' | b'\t' | b'\n' | b'\r' | b'>') => true,
        Some(b'/') if bytes.get(cursor + 1) == Some(&b'>') => true,
        _ => false,
    }
}

fn html_block_tag(tag: &str) -> bool {
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "address"
            | "article"
            | "aside"
            | "base"
            | "basefont"
            | "blockquote"
            | "body"
            | "caption"
            | "center"
            | "col"
            | "colgroup"
            | "dd"
            | "details"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "frame"
            | "frameset"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hr"
            | "html"
            | "iframe"
            | "legend"
            | "li"
            | "link"
            | "main"
            | "menu"
            | "menuitem"
            | "nav"
            | "noframes"
            | "ol"
            | "optgroup"
            | "option"
            | "p"
            | "param"
            | "search"
            | "section"
            | "summary"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "title"
            | "tr"
            | "track"
            | "ul"
    )
}

fn is_declaration_start(input: &str) -> bool {
    input
        .as_bytes()
        .get(2)
        .is_some_and(|byte| input.starts_with("<!") && byte.is_ascii_alphabetic())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MdxEsmState {
    brace_depth: usize,
    bracket_depth: usize,
    paren_depth: usize,
    block_comment: bool,
    quote: Option<u8>,
    escaped: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MdxJsxTag<'a> {
    Fragment,
    Named(&'a str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MdxJsxTagStart<'a> {
    tag: MdxJsxTag<'a>,
    closing: bool,
}

fn is_mdx_esm_start(line: &str) -> bool {
    line.starts_with("import ") || line.starts_with("export ")
}

fn is_mdx_esm_continuation(line: &str, state: &MdxEsmState) -> bool {
    if state_has_open_mdx_esm_construct(state) {
        return true;
    }
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return false;
    }
    is_mdx_esm_start(line) || trimmed.starts_with("//") || trimmed.starts_with("/*")
}

fn state_has_open_mdx_esm_construct(state: &MdxEsmState) -> bool {
    state.brace_depth > 0
        || state.bracket_depth > 0
        || state.paren_depth > 0
        || state.block_comment
        || state.quote == Some(b'`')
}

fn update_mdx_esm_state(line: &str, state: &mut MdxEsmState) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if state.block_comment {
            if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                state.block_comment = false;
                index += 1;
            }
            index += 1;
            continue;
        }

        if let Some(delimiter) = state.quote {
            if state.escaped {
                state.escaped = false;
            } else if byte == b'\\' {
                state.escaped = true;
            } else if byte == delimiter {
                state.quote = None;
            }
            index += 1;
            continue;
        }

        match byte {
            b'\'' | b'"' | b'`' => state.quote = Some(byte),
            b'/' if bytes.get(index + 1) == Some(&b'/') => break,
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                state.block_comment = true;
                index += 1;
            }
            b'{' => state.brace_depth += 1,
            b'}' => state.brace_depth = state.brace_depth.saturating_sub(1),
            b'[' => state.bracket_depth += 1,
            b']' => state.bracket_depth = state.bracket_depth.saturating_sub(1),
            b'(' => state.paren_depth += 1,
            b')' => state.paren_depth = state.paren_depth.saturating_sub(1),
            _ => {}
        }
        index += 1;
    }
}
/// Lexical states of the walk to an inline MDX expression's closing `}`.
const MDX_BRACE_STATES: usize = 6;
const MDX_NORMAL: usize = 0;
const MDX_SINGLE_QUOTED: usize = 1;
const MDX_DOUBLE_QUOTED: usize = 2;
const MDX_TEMPLATE: usize = 3;
const MDX_LINE_COMMENT: usize = 4;
const MDX_BLOCK_COMMENT: usize = 5;

/// One step of the walk to an inline MDX expression's closing `}`, over nodes
/// `byte position * MDX_BRACE_STATES + lexical state`: braces nest outside
/// strings and comments, and `\` escapes the next byte inside strings.
/// The position after the byte a `\` at `escape` escapes. In block lines joined
/// by `\n` (`lines_joined`), a `\` ending a line escapes the first byte of the
/// next non-empty line, as the line-by-line scan it stands for does, rather
/// than the line break.
fn after_escaped_byte(text: &str, escape: usize, lines_joined: bool) -> usize {
    let mut escaped = escape + 1;
    if lines_joined {
        while text.as_bytes().get(escaped) == Some(&b'\n') {
            escaped += 1;
        }
    }
    (escaped + 1).min(text.len())
}

fn mdx_expression_step(input: &str, node: usize, lines_joined: bool) -> BracketStep {
    let bytes = input.as_bytes();
    let (cursor, state) = (node / MDX_BRACE_STATES, node % MDX_BRACE_STATES);
    let at = |position: usize, state: usize| position.min(bytes.len()) * MDX_BRACE_STATES + state;
    let Some(&byte) = bytes.get(cursor) else {
        return BracketStep::End;
    };
    let following = bytes.get(cursor + 1).copied();
    match state {
        MDX_NORMAL => match byte {
            b'\'' => BracketStep::Pass(at(cursor + 1, MDX_SINGLE_QUOTED)),
            b'"' => BracketStep::Pass(at(cursor + 1, MDX_DOUBLE_QUOTED)),
            b'`' => BracketStep::Pass(at(cursor + 1, MDX_TEMPLATE)),
            b'/' if following == Some(b'/') => BracketStep::Pass(at(cursor + 2, MDX_LINE_COMMENT)),
            b'/' if following == Some(b'*') => BracketStep::Pass(at(cursor + 2, MDX_BLOCK_COMMENT)),
            b'{' => BracketStep::Open(at(cursor + 1, MDX_NORMAL)),
            b'}' => BracketStep::Close(at(cursor + 1, MDX_NORMAL)),
            _ => BracketStep::Pass(at(cursor + 1, MDX_NORMAL)),
        },
        MDX_LINE_COMMENT => BracketStep::Pass(at(
            cursor + 1,
            if byte == b'\n' {
                MDX_NORMAL
            } else {
                MDX_LINE_COMMENT
            },
        )),
        MDX_BLOCK_COMMENT => {
            if byte == b'*' && following == Some(b'/') {
                BracketStep::Pass(at(cursor + 2, MDX_NORMAL))
            } else {
                BracketStep::Pass(at(cursor + 1, MDX_BLOCK_COMMENT))
            }
        }
        quoted => {
            let delimiter = [b'\'', b'"', b'`'][quoted - MDX_SINGLE_QUOTED];
            if byte == b'\\' {
                BracketStep::Pass(at(after_escaped_byte(input, cursor, lines_joined), quoted))
            } else if byte == delimiter {
                BracketStep::Pass(at(cursor + 1, MDX_NORMAL))
            } else {
                BracketStep::Pass(at(cursor + 1, quoted))
            }
        }
    }
}

fn collect_mdx_expression_value(
    lines: &[Line<'_>],
    start_line: usize,
    open_byte: usize,
    close_line: usize,
    close_byte: usize,
) -> String {
    let mut value = String::new();
    let mut cursor = start_line;
    while cursor <= close_line {
        if cursor > start_line {
            value.push('\n');
        }
        let line = lines[cursor].text;
        let segment = if cursor == start_line && cursor == close_line {
            &line[open_byte + 1..close_byte]
        } else if cursor == start_line {
            &line[open_byte + 1..]
        } else if cursor == close_line {
            &line[..close_byte]
        } else {
            line
        };
        value.push_str(segment);
        cursor += 1;
    }
    value
}

fn mdx_jsx_tag_start(input: &str, start: usize) -> Option<MdxJsxTagStart<'_>> {
    let bytes = input.as_bytes();
    if bytes.get(start) != Some(&b'<') {
        return None;
    }

    match bytes.get(start + 1) {
        Some(b'>') => {
            return Some(MdxJsxTagStart {
                tag: MdxJsxTag::Fragment,
                closing: false,
            });
        }
        Some(b'/') if bytes.get(start + 2) == Some(&b'>') => {
            return Some(MdxJsxTagStart {
                tag: MdxJsxTag::Fragment,
                closing: true,
            });
        }
        Some(b'!' | b'?') | None => return None,
        _ => {}
    }

    let closing = bytes.get(start + 1) == Some(&b'/');
    let name_start = start + if closing { 2 } else { 1 };
    if !bytes
        .get(name_start)
        .is_some_and(|byte| is_mdx_jsx_name_start_byte(*byte))
    {
        return None;
    }

    let mut name_end = name_start + 1;
    while bytes
        .get(name_end)
        .is_some_and(|byte| is_mdx_jsx_name_byte(*byte))
    {
        name_end += 1;
    }
    if name_end == name_start {
        return None;
    }
    if bytes
        .get(name_end)
        .is_some_and(|byte| !is_mdx_jsx_name_delimiter(*byte))
    {
        return None;
    }
    Some(MdxJsxTagStart {
        tag: MdxJsxTag::Named(&input[name_start..name_end]),
        closing,
    })
}

fn previous_nonspace_before_text(input: &str, byte_index: usize) -> Option<u8> {
    input.as_bytes()[..byte_index]
        .iter()
        .rev()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
}

/// Quote states of the walk to an MDX JSX tag's closing `>`: outside quotes,
/// inside `'…'`, inside `"…"`.
const JSX_TAG_QUOTE_STATES: usize = 3;

/// One step of the walk to an MDX JSX tag's closing `>`, over nodes
/// `byte position * JSX_TAG_QUOTE_STATES + quote state`: quoted attribute
/// values and `{…}` expressions (stepped over whole via `expression_close`)
/// cannot close the tag.
fn jsx_tag_end_step(
    text: &str,
    node: usize,
    lines_joined: bool,
    expression_close: &mut impl FnMut(usize) -> Option<usize>,
) -> Step {
    let (cursor, quote) = (node / JSX_TAG_QUOTE_STATES, node % JSX_TAG_QUOTE_STATES);
    let at = |position: usize, quote: usize| Step::Next(position * JSX_TAG_QUOTE_STATES + quote);
    let Some(&byte) = text.as_bytes().get(cursor) else {
        return Step::Done(None);
    };
    if quote != 0 {
        let delimiter = if quote == 1 { b'\'' } else { b'"' };
        return if byte == b'\\' {
            at(after_escaped_byte(text, cursor, lines_joined), quote)
        } else if byte == delimiter {
            at(cursor + 1, 0)
        } else {
            at(cursor + 1, quote)
        };
    }
    match byte {
        b'\'' => at(cursor + 1, 1),
        b'"' => at(cursor + 1, 2),
        b'{' => match expression_close(cursor) {
            Some(close) => at(close + 1, 0),
            None => Step::Done(None),
        },
        b'>' => Step::Done(Some(cursor)),
        _ => at(cursor + 1, 0),
    }
}

/// Closing-tag lookups for MDX JSX over one text. A tag's closing tag is found
/// by walking from tag to tag (each stepped over whole, to its `>`) and
/// counting only tags with its name. Every tag is indexed once: where it ends,
/// and the next tag with its name along the walk after it, read from
/// persistent per-name maps built from the last tag back. Matching then walks
/// only same-name tags, memoized, so all closing-tag questions about one text
/// cost `O(n log n)` together.
struct JsxIndex {
    starts: Vec<usize>,
    closing: Vec<bool>,
    ends: Vec<Option<usize>>,
    self_closing: Vec<bool>,
    /// Per tag, the index of the next tag with its name along the walk after
    /// it, or `starts.len()` when the walk ends (or reaches a tag without an
    /// end) first.
    next_same_name: Vec<usize>,
    matches: BracketMemo,
}

impl JsxIndex {
    fn new(text: &str, lines_joined: bool, expressions: &mut BracketMemo) -> Self {
        let bytes = text.as_bytes();
        let mut starts = Vec::new();
        let mut closing = Vec::new();
        let mut names = Vec::new();
        for (position, byte) in bytes.iter().enumerate() {
            if *byte == b'<' {
                if let Some(tag) = mdx_jsx_tag_start(text, position) {
                    starts.push(position);
                    closing.push(tag.closing);
                    names.push(match tag.tag {
                        MdxJsxTag::Fragment => None,
                        MdxJsxTag::Named(name) => Some(name),
                    });
                }
            }
        }

        let mut tag_ends = PathMemo::default();
        let mut self_closing_by_end = BTreeMap::new();
        let mut ends = Vec::with_capacity(starts.len());
        let mut self_closing = Vec::with_capacity(starts.len());
        for &start in &starts {
            let end = tag_ends.resolve(
                (text.len() + 1) * JSX_TAG_QUOTE_STATES,
                (start + 1) * JSX_TAG_QUOTE_STATES,
                |node| {
                    jsx_tag_end_step(text, node, lines_joined, &mut |open| {
                        expressions
                            .resolve(
                                (text.len() + 1) * MDX_BRACE_STATES,
                                (open + 1) * MDX_BRACE_STATES,
                                |node| mdx_expression_step(text, node, lines_joined),
                            )
                            .map(|node| node / MDX_BRACE_STATES)
                    })
                },
            );
            ends.push(end);
            self_closing.push(end.is_some_and(|end| {
                *self_closing_by_end
                    .entry(end)
                    .or_insert_with(|| previous_nonspace_before_text(text, end) == Some(b'/'))
            }));
        }

        let mut name_ids = BTreeMap::new();
        for name in &names {
            let next_id = name_ids.len();
            name_ids.entry(*name).or_insert(next_id);
        }
        let count = starts.len();
        let mut maps = PersistentMap::new(name_ids.len());
        let mut versions = alloc::vec![PersistentMap::EMPTY; count];
        let mut next_same_name = alloc::vec![count; count];
        for tag in (0..count).rev() {
            // A tag without an end stops every walk that reaches it.
            let Some(end) = ends[tag] else {
                continue;
            };
            let after = starts.partition_point(|start| *start <= end);
            let rest = versions.get(after).copied().unwrap_or(PersistentMap::EMPTY);
            let name = name_ids[&names[tag]];
            next_same_name[tag] = maps.get(rest, name).unwrap_or(count);
            versions[tag] = maps.insert(rest, name, tag);
        }

        Self {
            starts,
            closing,
            ends,
            self_closing,
            next_same_name,
            matches: BracketMemo::default(),
        }
    }

    /// The `>` ending the element whose opening tag starts at `start`: its own
    /// `>` when self-closing, else the `>` of its matching closing tag.
    fn element_end(&mut self, start: usize) -> Option<usize> {
        let tag = self.starts.binary_search(&start).ok()?;
        if self.closing[tag] {
            return None;
        }
        let end = self.ends[tag]?;
        if self.self_closing[tag] {
            return Some(end);
        }
        let count = self.starts.len();
        let (closing, self_closing, next) =
            (&self.closing, &self.self_closing, &self.next_same_name);
        let close = self.matches.resolve(count + 1, next[tag], |tag| {
            if tag == count {
                BracketStep::End
            } else if closing[tag] {
                BracketStep::Close(next[tag])
            } else if self_closing[tag] {
                BracketStep::Pass(next[tag])
            } else {
                BracketStep::Open(next[tag])
            }
        })?;
        self.ends[close]
    }

    /// The `>` ending the tag that starts at `start`, and whether the tag is
    /// self-closing.
    fn tag_end(&self, start: usize) -> Option<(usize, bool)> {
        let tag = self.starts.binary_search(&start).ok()?;
        Some((self.ends[tag]?, self.self_closing[tag]))
    }
}

/// MDX flow lookups over one run of block lines, joined by `\n` into one text
/// on first use so JSX and expression closes are found by the same memoized
/// walks as inline ones.
#[derive(Default)]
struct MdxFlowScan {
    joined: Option<JoinedLines>,
}

struct JoinedLines {
    text: String,
    line_starts: Vec<usize>,
    expressions: BracketMemo,
    jsx: Option<JsxIndex>,
}

impl JoinedLines {
    fn new(lines: &[Line<'_>]) -> Self {
        let mut text = String::new();
        let mut line_starts = Vec::with_capacity(lines.len());
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            line_starts.push(text.len());
            text.push_str(line.text);
        }
        Self {
            text,
            line_starts,
            expressions: BracketMemo::default(),
            jsx: None,
        }
    }

    /// The line holding byte `position`, and the byte's offset in that line.
    fn locate(&self, position: usize) -> (usize, usize) {
        let line = self.line_starts.partition_point(|start| *start <= position) - 1;
        (line, position - self.line_starts[line])
    }

    fn jsx(&mut self) -> &mut JsxIndex {
        let Self {
            text,
            expressions,
            jsx,
            ..
        } = self;
        jsx.get_or_insert_with(|| JsxIndex::new(text, true, expressions))
    }
}

impl MdxFlowScan {
    fn joined(&mut self, lines: &[Line<'_>]) -> &mut JoinedLines {
        self.joined.get_or_insert_with(|| JoinedLines::new(lines))
    }

    /// The line and byte of the `}` closing the expression block opening at
    /// `open_byte` of line `index`, when nothing but whitespace follows it on
    /// its line.
    fn expression_close(
        &mut self,
        lines: &[Line<'_>],
        index: usize,
        open_byte: usize,
    ) -> Option<(usize, usize)> {
        let joined = self.joined(lines);
        let open = joined.line_starts[index] + open_byte;
        let text = &joined.text;
        let start = (open + 1) * MDX_BRACE_STATES;
        let close =
            joined
                .expressions
                .resolve((text.len() + 1) * MDX_BRACE_STATES, start, |node| {
                    mdx_expression_step(text, node, true)
                })?
                / MDX_BRACE_STATES;
        let (line, byte) = joined.locate(close);
        lines[line].text[byte + 1..]
            .trim()
            .is_empty()
            .then_some((line, byte))
    }

    /// The last line of the JSX element block opening at `start_byte` of line
    /// `index`.
    fn jsx_close_line(
        &mut self,
        lines: &[Line<'_>],
        index: usize,
        start_byte: usize,
    ) -> Option<usize> {
        let joined = self.joined(lines);
        let start = joined.line_starts[index] + start_byte;
        let end = joined.jsx().element_end(start)?;
        Some(joined.locate(end).0)
    }

    /// Whether the JSX tag opening at `start_byte` of line `index` ends, and if
    /// so whether it is self-closing.
    fn jsx_tag_self_closing(
        &mut self,
        lines: &[Line<'_>],
        index: usize,
        start_byte: usize,
    ) -> Option<bool> {
        let joined = self.joined(lines);
        let start = joined.line_starts[index] + start_byte;
        joined
            .jsx()
            .tag_end(start)
            .map(|(_, self_closing)| self_closing)
    }
}

fn is_mdx_jsx_name_start_byte(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

fn is_mdx_jsx_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b'_' | b'-' | b'$')
}

fn is_mdx_jsx_name_delimiter(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'/' | b'>' | b'{' | b'}')
}

fn collect_line_range(lines: &[Line<'_>], start: usize, end: usize) -> String {
    let mut value = String::new();
    let mut cursor = start;
    while cursor <= end {
        if cursor > start {
            value.push('\n');
        }
        value.push_str(lines[cursor].text);
        cursor += 1;
    }
    value
}

fn setext_underline_depth(input: &str) -> Option<u8> {
    let underline = trim_up_to_three_spaces(input)?.trim_matches([' ', '\t']);
    match underline {
        text if !text.is_empty() && text.chars().all(|char| char == '=') => Some(1),
        text if !text.is_empty() && text.chars().all(|char| char == '-') => Some(2),
        _ => None,
    }
}

/// A delimiter run recorded during the inline scan for later resolution by the
/// delimiter-stack algorithm (`process_emphasis`).
///
/// Marks pair in one of two ways. *Nested* roles (`can_open` / `can_close`)
/// pair a closer with the nearest compatible opener before it, so same-mark
/// spans nest: `*` and `_` (CommonMark), `~~` strikethrough, `++` insert, and
/// `==` highlight. *First-closer* roles pair an opener with the first closer of
/// its kind on the same line and never nest: `||` spoiler (through `can_open`
/// / `can_close`) and the one-character `~` subscript and `^` superscript
/// (through `single_open` / `single_close`).
#[derive(Clone, Copy)]
struct DelimMarker {
    /// Index of the placeholder text node in the flat node list. The text node
    /// holds the as-yet-unmatched delimiter characters; matching trims it from
    /// the appropriate side and matched characters are removed entirely.
    node_index: usize,
    marker: u8,
    /// Remaining unmatched delimiter characters in this run.
    length: usize,
    /// The run's length as scanned. CommonMark's rule of three, and the
    /// `openers_bottom` key that caches its verdicts, read this rather than
    /// what is left after earlier pairings.
    run_length: usize,
    can_open: bool,
    can_close: bool,
    /// The `~` subscript / `^` superscript roles of a run.
    single_open: bool,
    single_close: bool,
    /// A `~` run that can pair as strikethrough.
    strike: bool,
    /// How many more times a long `++` / `==` run closes, two characters at a
    /// time, after closing with its first two.
    recloses: usize,
    /// The opener of the latest-starting mark span enclosing this run (see
    /// `assign_emphasis_roles`), or `NIL`.
    enclosed_from: usize,
    /// Offset of the run in the inline input, for adjacency checks.
    position: usize,
    /// How many line breaks precede the run in the inline input; first-closer
    /// spans cannot cross a line break.
    line: usize,
}

impl DelimMarker {
    /// Whether `can_open` / `can_close` are nested roles. A `||` spoiler uses
    /// them as first-closer roles instead.
    fn nests(&self) -> bool {
        self.marker != b'|'
    }
}

/// End-of-list marker for the index-linked lists used by `process_emphasis`.
const NIL: usize = usize::MAX;

/// The deepest `Emphasis`/`Strong`/`Delete` nesting one inline pass builds. A
/// delimiter pair that would nest deeper stays literal text, which bounds the
/// tree depth that recursive consumers (text merging, serialization, `Drop`)
/// walk.
const MAX_EMPHASIS_NESTING: usize = 16;

/// How deeply the nodes between two delimiter runs nest: `emphasis` counts
/// `Emphasis`/`Strong`/`Delete` levels since the nearest enclosed mark span,
/// bracket, or directive,
/// and `total` counts every inline container level (emphasis, marks, and
/// formed brackets and directives). `total` counts toward `MAX_INLINE_NESTING`
/// together with the inline passes and open brackets enclosing these nodes;
/// emphasis counts toward `MAX_EMPHASIS_NESTING` within one mark.
#[derive(Clone, Copy, Default)]
struct Nesting {
    emphasis: usize,
    total: usize,
}

impl Nesting {
    fn max(self, other: Self) -> Self {
        Self {
            emphasis: self.emphasis.max(other.emphasis),
            total: self.total.max(other.total),
        }
    }
}

/// The delimiter stack `process_emphasis` works on: live runs form a doubly
/// linked list in source order, and each live run records the deepest nesting
/// among the nodes between it and the next live run.
struct DelimiterLinks {
    prev: Vec<usize>,
    next: Vec<usize>,
    live: Vec<bool>,
    nesting_after: Vec<Nesting>,
}

impl DelimiterLinks {
    fn new(count: usize) -> Self {
        Self {
            prev: (0..count).map(|index| index.wrapping_sub(1)).collect(),
            next: (1..=count)
                .map(|index| if index < count { index } else { NIL })
                .collect(),
            live: alloc::vec![true; count],
            nesting_after: alloc::vec![Nesting::default(); count],
        }
    }

    /// Unlinks `index`; the nodes after it now follow the previous live run.
    fn unlink(&mut self, index: usize) {
        let (before, after) = (self.prev[index], self.next[index]);
        self.live[index] = false;
        if before != NIL {
            self.next[before] = after;
            self.nesting_after[before] = self.nesting_after[before].max(self.nesting_after[index]);
        }
        if after != NIL {
            self.prev[after] = before;
        }
    }

    /// Unlinks every run strictly between `opener` and `closer` and returns the
    /// deepest nesting among the nodes between them.
    fn close_span(&mut self, opener: usize, closer: usize) -> Nesting {
        let mut nesting = self.nesting_after[opener];
        let mut inner = self.next[opener];
        while inner != closer {
            nesting = nesting.max(self.nesting_after[inner]);
            self.live[inner] = false;
            inner = self.next[inner];
        }
        self.next[opener] = closer;
        self.prev[closer] = opener;
        nesting
    }
}

/// The flat inline list `process_emphasis` rewrites, held as a doubly linked
/// list over stable slots. Wrapping a matched span and dropping a consumed
/// delimiter run cost time proportional to the wrapped nodes rather than to the
/// whole list, and delimiter `node_index` values never need re-indexing.
struct InlineList {
    slots: Vec<Option<Inline>>,
    prev: Vec<usize>,
    next: Vec<usize>,
    head: usize,
}

impl InlineList {
    fn new(nodes: Vec<Inline>) -> Self {
        let count = nodes.len();
        Self {
            slots: nodes.into_iter().map(Some).collect(),
            prev: (0..count).map(|slot| slot.wrapping_sub(1)).collect(),
            next: (1..=count)
                .map(|slot| if slot < count { slot } else { NIL })
                .collect(),
            head: if count == 0 { NIL } else { 0 },
        }
    }

    fn node_mut(&mut self, slot: usize) -> &mut Inline {
        self.slots[slot]
            .as_mut()
            .expect("delimiter placeholder slot is live")
    }

    /// Replaces every node strictly between `before` and `after` with the node
    /// `wrap` builds from them (in order).
    fn wrap_between(
        &mut self,
        before: usize,
        after: usize,
        wrap: impl FnOnce(Vec<Inline>) -> Inline,
    ) {
        let mut children = Vec::new();
        let mut cursor = self.next[before];
        while cursor != after {
            children.push(self.slots[cursor].take().expect("linked slot is live"));
            cursor = self.next[cursor];
        }
        let slot = self.slots.len();
        self.slots.push(Some(wrap(children)));
        self.prev.push(before);
        self.next.push(after);
        self.next[before] = slot;
        self.prev[after] = slot;
    }

    fn remove(&mut self, slot: usize) {
        let (prev, next) = (self.prev[slot], self.next[slot]);
        if prev == NIL {
            self.head = next;
        } else {
            self.next[prev] = next;
        }
        if next != NIL {
            self.prev[next] = prev;
        }
        self.slots[slot] = None;
    }

    fn into_vec(mut self) -> Vec<Inline> {
        let mut nodes = Vec::new();
        let mut cursor = self.head;
        while cursor != NIL {
            nodes.push(self.slots[cursor].take().expect("linked slot is live"));
            cursor = self.next[cursor];
        }
        nodes
    }
}

/// Counts the line breaks before each recorded run. Runs are recorded in
/// source order, so the count advances incrementally.
#[derive(Default)]
struct LineCounter {
    position: usize,
    line: usize,
}

impl LineCounter {
    fn line_at(&mut self, input: &str, position: usize) -> usize {
        self.line += input.as_bytes()[self.position..position]
            .iter()
            .filter(|byte| matches!(byte, b'\n' | b'\r'))
            .count();
        self.position = position;
        self.line
    }
}

/// The roles a recorded run can play; see `DelimMarker`.
#[derive(Clone, Copy, Default)]
struct DelimRoles {
    can_open: bool,
    can_close: bool,
    single_open: bool,
    single_close: bool,
    /// A `~` run that can pair as strikethrough; its roles are settled by
    /// `assign_emphasis_roles`.
    strike: bool,
    recloses: usize,
}

/// Pushes a literal text node for `value` starting at `start` of the block-level
/// inline input.
fn push_text(nodes: &mut Vec<Inline>, start: usize, value: &str) {
    nodes.push(Inline::Text(Text {
        meta: NodeMeta::new(Some(Span::new(start, start + value.len()))),
        value: value.into(),
    }));
}

/// Resolves the `]` at `close` against the innermost open bracket. Returns the
/// end of the construct it forms, or `None` when the `]` stays literal text
/// (the opener's characters then stay literal too).
#[allow(clippy::too_many_arguments)]
fn close_bracket(
    input: &str,
    base_offset: usize,
    close: usize,
    options: &SyntaxOptions,
    definitions: Option<Definitions<'_>>,
    nodes: &mut Vec<Inline>,
    delimiters: &mut Vec<DelimMarker>,
    brackets: &mut Brackets,
    pass: &mut InlinePass,
) -> Option<usize> {
    let mut opener = brackets.openers.pop()?;
    let depth = pass.state.depth + brackets.openers.len();
    // The opener's own stack slot decides whether it may form a link; later
    // openers pushed into that slot start out able to.
    let link_active = brackets.openers.len() >= brackets.link_floor;
    brackets.link_floor = brackets.link_floor.min(brackets.openers.len());

    if opener.kind == BracketKind::InlineFootnote {
        if !input[opener.position + 2..close].trim().is_empty() {
            let (children, marks) =
                take_label(input, options, nodes, delimiters, brackets, &opener, depth);
            nodes.truncate(opener.node_index - 1);
            if let Some(caret) = opener.caret {
                delimiters.truncate(caret);
            }
            nodes.push(Inline::InlineFootnote(InlineFootnote {
                meta: NodeMeta::new(Some(Span::new(
                    base_offset + opener.position,
                    base_offset + close + 1,
                ))),
                children,
            }));
            brackets.formed.push((nodes.len() - 1, marks + 1));
            return Some(close + 1);
        }
        // An empty inline footnote is a `^` that may close a superscript, then
        // a plain `[`.
        if let Some(caret) = opener.caret {
            delimiters[caret].single_close = true;
        }
        opener.kind = BracketKind::Link;
        opener.position += 1;
    }

    if opener.kind == BracketKind::Image {
        let target = match_link_target(
            &mut pass.scan,
            input,
            opener.position + 2,
            close,
            definitions,
        );
        if let Some((end, target)) = target {
            let (children, marks) =
                take_label(input, options, nodes, delimiters, brackets, &opener, depth);
            nodes.truncate(opener.node_index - 1);
            let span = Span::new(base_offset + opener.position, base_offset + end);
            nodes.push(link_node(target, true, span, children));
            brackets.formed.push((nodes.len() - 1, marks + 1));
            return Some(end);
        }
        // Not an image: the `!` stays text and the `[` may still open a link.
        opener.kind = BracketKind::Link;
        opener.position += 1;
    }

    if link_active {
        let target = match_link_target(
            &mut pass.scan,
            input,
            opener.position + 1,
            close,
            definitions,
        );
        if let Some((end, target)) = target {
            let (children, marks) =
                take_label(input, options, nodes, delimiters, brackets, &opener, depth);
            nodes.truncate(opener.node_index);
            let span = Span::new(base_offset + opener.position, base_offset + end);
            nodes.push(link_node(target, false, span, children));
            brackets.formed.push((nodes.len() - 1, marks + 1));
            // Links do not nest: no earlier `[` forms a link around this one.
            brackets.close_links();
            return Some(end);
        }
    }

    // `[^label]` is a footnote reference when the label is plain: no link or
    // image formed inside it.
    let holds_formed = brackets
        .formed
        .last()
        .is_some_and(|&(node, _)| node > opener.node_index);
    if options.constructs.footnote_reference
        && !holds_formed
        && input[opener.position + 1..].starts_with('^')
    {
        let label = &input[opener.position + 2..close];
        if is_footnote_label(label) {
            drop_label(nodes, delimiters, brackets, &opener);
            nodes.push(Inline::FootnoteReference(FootnoteReference {
                meta: NodeMeta::new(Some(Span::new(
                    base_offset + opener.position,
                    base_offset + close + 1,
                ))),
                label: label.into(),
                identifier: normalize_label(label),
            }));
            return Some(close + 1);
        }
    }
    None
}

/// Drops `opener`'s `[` and everything recorded after it.
fn drop_label(
    nodes: &mut Vec<Inline>,
    delimiters: &mut Vec<DelimMarker>,
    brackets: &mut Brackets,
    opener: &BracketOpener,
) {
    nodes.truncate(opener.node_index);
    delimiters.truncate(opener.delimiter_bottom);
    let kept = brackets
        .formed
        .partition_point(|&(node, _)| node < opener.node_index);
    brackets.formed.truncate(kept);
}

/// Pushes the `length`-byte run of `marker` at `index` as a literal text node
/// plus a delimiter entry with `roles`.
#[allow(clippy::too_many_arguments)]
fn push_delimiter(
    nodes: &mut Vec<Inline>,
    delimiters: &mut Vec<DelimMarker>,
    lines: &mut LineCounter,
    input: &str,
    index: usize,
    base_offset: usize,
    marker: u8,
    length: usize,
    roles: DelimRoles,
) {
    let node_index = nodes.len();
    nodes.push(Inline::Text(Text {
        meta: NodeMeta::new(Some(Span::new(
            base_offset + index,
            base_offset + index + length,
        ))),
        value: String::from(marker as char).repeat(length),
    }));
    delimiters.push(DelimMarker {
        node_index,
        marker,
        length,
        run_length: length,
        can_open: roles.can_open,
        can_close: roles.can_close,
        single_open: roles.single_open,
        single_close: roles.single_close,
        strike: roles.strike,
        recloses: roles.recloses,
        enclosed_from: NIL,
        position: index,
        line: lines.line_at(input, index),
    });
}

/// The CommonMark roles of a `*`/`_`/`~~` run.
///
/// Flanking is computed on the whole run (CommonMark treats left/right-flanking
/// as a property of the run, not of an individual delimiter), including the
/// `_` intraword punctuation rules.
///
/// `strikethrough` enables the GFM cross-marker bonus: when strikethrough is an
/// active construct, a `*` run immediately adjacent to a `~` counts as
/// openable/closeable even though `~` is a punctuation character (this is what
/// makes `a*~b~*c` emphasize). A `_` run or a `~` run gets plain CommonMark
/// flanking beside a `~`.
///
/// A side that touches the boundary of an enclosing mark span (`bounded_before`
/// / `bounded_after`) flanks as the end of the span's content.
fn emphasis_roles(
    input: &str,
    index: usize,
    length: usize,
    marker: u8,
    strikethrough: bool,
    bounded_before: bool,
    bounded_after: bool,
) -> (bool, bool) {
    let flanking = delimiter_flanking_within(input, index, length, bounded_before, bounded_after);
    let (mut can_open, mut can_close) = if marker == b'_' {
        (
            flanking.left
                && (!flanking.right || flanking.previous.is_some_and(is_flanking_punctuation)),
            flanking.right
                && (!flanking.left || flanking.next.is_some_and(is_flanking_punctuation)),
        )
    } else {
        (flanking.left, flanking.right)
    };

    // GFM: a `*` run touching a `~` strikethrough marker may open/close even
    // when ordinary flanking refuses it (the `~` would otherwise be a blocking
    // punctuation neighbour).
    if strikethrough && marker == b'*' {
        if flanking.next == Some('~') {
            can_open = true;
        }
        if flanking.previous == Some('~') {
            can_close = true;
        }
    }
    (can_open, can_close)
}

/// Settles the roles of every `*`, `_`, and `~~` run once the mark spans are
/// known. The mark runs (`++`, `==`, `||`, `~`, `^`, and with `underline` the
/// `__` runs) are paired among themselves first; an emphasis run touching the
/// inner side of such a span's delimiter then flanks as the edge of that span's
/// content.
fn assign_emphasis_roles(
    input: &str,
    delimiters: &mut [DelimMarker],
    strikethrough: bool,
    underline: bool,
) {
    let underline_mark = |run: &DelimMarker| underline && run.marker == b'_' && run.length == 2;
    let mut opens_span = alloc::vec![false; delimiters.len()];
    let mut closes_span = alloc::vec![false; delimiters.len()];
    let mut open_marks: [Vec<usize>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut pending: [Option<usize>; 3] = [None; 3];
    // The run each span opened by a run closes at (a run opens at most one).
    let mut span_close = alloc::vec![NIL; delimiters.len()];
    for (index, run) in delimiters.iter().enumerate() {
        if underline_mark(run) {
            let open = &mut open_marks[2];
            if can_close_underscore(input, run.position, 2) {
                if let Some(opener) = open.pop() {
                    opens_span[opener] = true;
                    closes_span[index] = true;
                    span_close[opener] = index;
                    continue;
                }
            }
            if can_open_underscore(input, run.position, 2) {
                open.push(index);
            }
            continue;
        }
        if matches!(run.marker, b'+' | b'=') {
            let open = &mut open_marks[usize::from(run.marker == b'=')];
            let mut closed = 0;
            if run.can_close {
                while closed <= run.recloses {
                    let Some(opener) = open.pop() else {
                        break;
                    };
                    opens_span[opener] = true;
                    closes_span[index] = true;
                    span_close[opener] = index;
                    closed += 1;
                }
            }
            // What is left of the run after closing may still open.
            if run.can_open && run.length >= 2 * closed + 2 {
                open.push(index);
            }
            continue;
        }
        let Some(kind) = first_closer_kind(run) else {
            continue;
        };
        if let Some(opener) = pending[kind] {
            let opener_run = &delimiters[opener];
            if opener_run.line != run.line {
                pending[kind] = None;
            } else if first_closer_can_close(run) {
                pending[kind] = None;
                if opener_run.position + opener_run.length != run.position {
                    opens_span[opener] = true;
                    closes_span[index] = true;
                    span_close[opener] = index;
                    // A long `||` run opens again with what it has left.
                    if run.marker == b'|' && run.can_open && run.length >= 4 {
                        pending[kind] = Some(index);
                    }
                    continue;
                }
            }
        }
        if pending[kind].is_none() && first_closer_can_open(run) {
            pending[kind] = Some(index);
        }
    }

    for index in 0..delimiters.len() {
        let run = delimiters[index];
        if !matches!(run.marker, b'*' | b'_') && !run.strike {
            continue;
        }
        let bounded_before = index > 0 && {
            let before = &delimiters[index - 1];
            before.position + before.length == run.position && opens_span[index - 1]
        };
        let bounded_after = index + 1 < delimiters.len() && {
            let after = &delimiters[index + 1];
            after.position == run.position + run.length && closes_span[index + 1]
        };
        let (can_open, can_close) = emphasis_roles(
            input,
            run.position,
            run.length,
            run.marker,
            strikethrough,
            bounded_before,
            bounded_after,
        );
        delimiters[index].can_open = can_open;
        delimiters[index].can_close = can_close;
    }

    // Each run's latest-starting enclosing mark span. Spans are pushed in
    // opener order, so the top of `open_spans` is the latest-starting one;
    // spans that have ended are dropped when they reach the top.
    let mut open_spans: Vec<usize> = Vec::new();
    for (index, run) in delimiters.iter_mut().enumerate() {
        while open_spans
            .last()
            .is_some_and(|&opener| span_close[opener] <= index)
        {
            open_spans.pop();
        }
        if let Some(&opener) = open_spans.last() {
            run.enclosed_from = opener;
        }
        if span_close[index] != NIL {
            open_spans.push(index);
        }
    }
}

/// The roles of a `++` / `==` run of `length` bytes at `index`: it opens with
/// its last two characters and closes with its first two (and then the next
/// two), each subject to CommonMark flanking of that two-character delimiter.
/// A run right after an escaped character of the same mark never closes.
fn double_mark_roles(input: &str, index: usize, length: usize, marker: u8) -> DelimRoles {
    let after_escaped_mark =
        index > 0 && input.as_bytes()[index - 1] == marker && is_escaped_at(input, index - 1);
    let can_close = !after_escaped_mark && can_close_delimited(input, index, 2);
    let recloses = if can_close {
        (1..length / 2)
            .take_while(|pair| can_close_delimited(input, index + 2 * pair, 2))
            .count()
    } else {
        0
    };
    DelimRoles {
        can_open: can_open_delimited(input, index + length - 2, 2),
        can_close,
        recloses,
        ..DelimRoles::default()
    }
}

/// What resolving one delimiter pair needs to know beyond the runs themselves.
#[derive(Clone, Copy)]
struct EmphasisContext {
    /// `__` pairs form `Underline` instead of `Strong`.
    underline: bool,
    /// How many nesting levels enclose these runs: the inline passes around
    /// this one, plus the open brackets around a bracket label.
    depth: usize,
}

/// Resolves recorded delimiter runs into emphasis and mark nodes using the
/// delimiter-stack algorithm, leaving unmatched runs as text. Closers are taken
/// in source order; when a pair closes, runs strictly between its opener and
/// closer can no longer pair across it.
///
/// `formed` lists the link, image, footnote, and directive nodes already built
/// among `nodes`, with their nesting depth, so spans around them count it.
/// Returns the nodes and the deepest nesting among them.
fn process_emphasis(
    nodes: Vec<Inline>,
    mut delimiters: Vec<DelimMarker>,
    context: EmphasisContext,
    formed: &[(usize, usize)],
) -> (Vec<Inline>, usize) {
    let mut deepest = formed.iter().map(|&(_, depth)| depth).max().unwrap_or(0);
    if delimiters.is_empty() {
        let mut nodes = nodes;
        merge_adjacent_text(&mut nodes);
        return (nodes, deepest);
    }
    let mut nodes = InlineList::new(nodes);

    // A run that is consumed, demoted to plain text, or enclosed by a newly
    // closed span is unlinked, so the opener walk below only ever visits live
    // runs.
    let mut links = DelimiterLinks::new(delimiters.len());
    for &(node, depth) in formed {
        let before = delimiters.partition_point(|run| run.node_index < node);
        if before > 0 {
            let gap = &mut links.nesting_after[before - 1].total;
            *gap = (*gap).max(depth);
        }
    }

    // `openers_bottom` records, per (marker, opener-can-also-close, length % 3),
    // the lowest opener index a nested closer is allowed to reach. Closers below
    // this bound for their key have already been proven to have no compatible
    // opener.
    let mut openers_bottom: [Option<usize>; NESTED_MARKERS * 6] = [None; NESTED_MARKERS * 6];
    let mut span_bottom: Vec<Vec<(usize, usize)>> = Vec::new();
    // The opener each first-closer kind (`||`, `~`, `^`) is waiting to close.
    let mut pending: [Option<usize>; 3] = [None; 3];
    let mut closer_idx = 0;
    // The closer whose first-closer bookkeeping has already run, so a closer
    // revisited for leftover delimiters does not run it twice.
    let mut visited = NIL;

    // Every run at or after `closer_idx` is still linked: runs are only ever
    // unlinked at or before the current closer.
    while closer_idx < delimiters.len() {
        let closer = delimiters[closer_idx];

        let mut first = None;
        let kind = first_closer_kind(&closer);
        if let Some(kind) = kind {
            if visited != closer_idx {
                if let Some(opener) = pending[kind] {
                    let opener_run = delimiters[opener];
                    let usable = links.live[opener]
                        && first_closer_can_open(&opener_run)
                        && opener_run.line == closer.line;
                    if !usable {
                        pending[kind] = None;
                    } else if first_closer_can_close(&closer) {
                        if opener_run.position + opener_run.length == closer.position {
                            // An empty span: the opener fails for good.
                            pending[kind] = None;
                        } else {
                            first = Some(opener);
                        }
                    }
                }
            }
        }
        visited = closer_idx;

        // A nested opener wins only when it is nearer than the first-closer
        // opener, so the search stops there.
        let mut nested = None;
        if closer.can_close && closer.nests() {
            nested = nested_opener(
                &delimiters,
                &links,
                &openers_bottom,
                &span_bottom,
                closer_idx,
                first.map(|opener| opener + 1),
            );
        }

        let choice = match (nested, first) {
            (Some(nested), Some(first)) => Some((nested.max(first), nested > first)),
            (Some(nested), None) => Some((nested, true)),
            (None, Some(first)) => Some((first, false)),
            (None, None) => None,
        };

        let Some((opener_idx, is_nested)) = choice else {
            if closer.can_close && closer.nests() {
                // No opener found: remember how far we searched so future
                // closers of the same key skip the same dead range.
                let key = openers_bottom_key(&closer);
                match span_floor(&closer) {
                    Some(span) => {
                        if span_bottom.len() <= span {
                            span_bottom.resize_with(span + 1, Vec::new);
                        }
                        match span_bottom[span].iter_mut().find(|(k, _)| *k == key) {
                            Some(bound) => bound.1 = closer_idx,
                            None => span_bottom[span].push((key, closer_idx)),
                        }
                    }
                    None => openers_bottom[key] = Some(closer_idx),
                }
            }
            if let Some(kind) = kind {
                if pending[kind].is_none() && first_closer_can_open(&closer) {
                    pending[kind] = Some(closer_idx);
                }
            }
            if !closer.can_open && !closer.single_open {
                links.unlink(closer_idx);
            }
            closer_idx += 1;
            continue;
        };

        if !is_nested {
            if let Some(kind) = kind {
                pending[kind] = None;
            }
        }
        let (used, wrap) = pair_shape(
            &delimiters[opener_idx],
            &delimiters[closer_idx],
            is_nested,
            context,
        );

        // Drop delimiters strictly between the opener and closer: they could not
        // match outward across this newly closed span.
        let inner = links.close_span(opener_idx, closer_idx);
        let fits = context.depth + inner.total < MAX_INLINE_NESTING
            && (wrap.is_mark() || inner.emphasis < MAX_EMPHASIS_NESTING);
        if fits {
            apply_emphasis(
                &mut nodes,
                &mut delimiters,
                opener_idx,
                closer_idx,
                used,
                wrap,
            );
            links.nesting_after[opener_idx] = Nesting {
                emphasis: if wrap.is_mark() {
                    0
                } else {
                    inner.emphasis + 1
                },
                total: inner.total + 1,
            };
            deepest = deepest.max(inner.total + 1);
        } else {
            // Too deep to wrap: both runs stay whole in their text nodes as
            // literal text and pair no further. Any span enclosing this one is
            // at least as deep, so it stays literal too.
            delimiters[opener_idx].length = 0;
            delimiters[closer_idx].length = 0;
            links.nesting_after[opener_idx] = inner;
        }
        if matches!(delimiters[opener_idx].marker, b'+' | b'=' | b'|') {
            // These runs open only with their last two characters.
            delimiters[opener_idx].can_open = false;
        }
        if is_nested && matches!(closer.marker, b'+' | b'=') {
            // A long run closes a second time only with its next two
            // characters.
            let run = &mut delimiters[closer_idx];
            run.can_close = run.recloses > 0;
            run.recloses = run.recloses.saturating_sub(1);
        }
        if !is_nested {
            // A run consumed as a one-character subscript/superscript closer
            // leaves its other characters literal.
            if closer.marker == b'~' {
                delimiters[closer_idx].can_open = false;
                delimiters[closer_idx].can_close = false;
            }
            delimiters[closer_idx].single_close = false;
        }

        if delimiters[opener_idx].length == 0 {
            links.unlink(opener_idx);
        }
        let leftover = delimiters[closer_idx];
        if leftover.length == 0 {
            links.unlink(closer_idx);
            closer_idx += 1;
        } else if !is_nested {
            // A first-closer run whose leftover can still open waits for the
            // next closer of its kind.
            if let Some(kind) = kind {
                if first_closer_can_open(&leftover) {
                    pending[kind] = Some(closer_idx);
                }
            }
            if !first_closer_can_open(&leftover) {
                links.unlink(closer_idx);
            }
            closer_idx += 1;
        }
        // When a nested closer still has delimiters left it stays the active
        // closer so the leftover can match an earlier opener (e.g. `***foo*`
        // keeps `**`).
    }

    // Adjacent text nodes can appear where unmatched delimiter runs ended up
    // beside literal text (`**foo*bar*` -> `**foo` + emphasis). CommonMark
    // coalesces them as the final step; do the same for the spans we created.
    let mut nodes = nodes.into_vec();
    merge_adjacent_text(&mut nodes);
    (nodes, deepest)
}

/// The markers that pair through nested roles, in `openers_bottom` order.
const NESTED_MARKERS: usize = 5;

/// The nearest live opener before `closer_idx` that the nested closer there can
/// pair with, searching no lower than its `openers_bottom` bound.
///
/// A closer that can also open does not close across a mark span enclosing it:
/// it searches no lower than that span's opener, bounded per span by
/// `span_bottom` the way `openers_bottom` bounds the whole list. `floor` stops
/// the search at a nearer first-closer opener.
fn nested_opener(
    delimiters: &[DelimMarker],
    links: &DelimiterLinks,
    openers_bottom: &[Option<usize>],
    span_bottom: &[Vec<(usize, usize)>],
    closer_idx: usize,
    floor: Option<usize>,
) -> Option<usize> {
    let closer = &delimiters[closer_idx];
    let key = openers_bottom_key(closer);
    let mut bottom = openers_bottom[key].max(floor);
    if let Some(span) = span_floor(closer) {
        let span_floor = span_bottom
            .get(span)
            .and_then(|bounds| bounds.iter().find(|(k, _)| *k == key))
            .map_or(span + 1, |&(_, bound)| bound);
        bottom = bottom.max(Some(span_floor));
    }
    let mut search = links.prev[closer_idx];
    while search != NIL {
        if bottom.is_some_and(|bottom| search < bottom) {
            return None;
        }
        let candidate = &delimiters[search];
        if candidate.marker == closer.marker
            && candidate.can_open
            && emphasis_delimiters_match(candidate, closer)
        {
            return Some(search);
        }
        search = links.prev[search];
    }
    None
}

/// The enclosing mark span a nested closer may not close across: only a run
/// that can also open is held inside it.
fn span_floor(closer: &DelimMarker) -> Option<usize> {
    (closer.can_open && closer.enclosed_from != NIL).then_some(closer.enclosed_from)
}

/// The `pending` slot of a run with first-closer roles: `||`, `~`, or `^`.
fn first_closer_kind(run: &DelimMarker) -> Option<usize> {
    match run.marker {
        b'|' => Some(0),
        b'~' if run.single_open || run.single_close => Some(1),
        b'^' => Some(2),
        _ => None,
    }
}

fn first_closer_can_open(run: &DelimMarker) -> bool {
    if run.marker == b'|' {
        run.can_open && run.length >= 2
    } else {
        run.single_open
    }
}

fn first_closer_can_close(run: &DelimMarker) -> bool {
    if run.marker == b'|' {
        run.can_close && run.length >= 2
    } else {
        run.single_close
    }
}

/// How many characters a pair consumes from each run, and the node it forms.
fn pair_shape(
    opener: &DelimMarker,
    closer: &DelimMarker,
    nested: bool,
    context: EmphasisContext,
) -> (usize, EmphasisWrap) {
    match closer.marker {
        b'~' if nested => {
            // Strikethrough consumes the whole (equal-length) run on each side at
            // once; the marker width selects the `Delete` flavour.
            let marker = if closer.length >= 2 {
                DeleteMarker::DoubleTilde
            } else {
                DeleteMarker::SingleTilde
            };
            (closer.length, EmphasisWrap::Delete(marker))
        }
        b'~' => (1, EmphasisWrap::Subscript),
        b'^' => (1, EmphasisWrap::Superscript),
        b'+' => (2, EmphasisWrap::Insert),
        b'=' => (2, EmphasisWrap::Mark),
        b'|' => (2, EmphasisWrap::Spoiler),
        marker => {
            let strong = opener.length >= 2 && closer.length >= 2;
            if !strong {
                (1, EmphasisWrap::Emphasis)
            } else if marker == b'_' && context.underline {
                (2, EmphasisWrap::Underline)
            } else {
                (2, EmphasisWrap::Strong)
            }
        }
    }
}

/// Merges consecutive `Text` nodes in a list, recursing into the emphasis and
/// mark nodes produced at this level. Other containers were already finalized
/// by their own `parse_inlines` pass and are left untouched.
fn merge_adjacent_text(nodes: &mut Vec<Inline>) {
    let mut write = 0;
    for read in 0..nodes.len() {
        if read != write {
            nodes.swap(read, write);
        }
        if write > 0 {
            let (head, tail) = nodes.split_at_mut(write);
            if let (Inline::Text(previous), Inline::Text(current)) =
                (&mut head[write - 1], &tail[0])
            {
                previous.value.push_str(&current.value);
                if let (Some(previous_span), Some(current_span)) =
                    (previous.meta.span.as_mut(), current.meta.span)
                {
                    previous_span.end = current_span.end;
                }
                continue;
            }
        }
        write += 1;
    }
    nodes.truncate(write);

    for node in nodes.iter_mut() {
        match node {
            Inline::Emphasis(node) => merge_adjacent_text(&mut node.children),
            Inline::Strong(node) => merge_adjacent_text(&mut node.children),
            Inline::Delete(node) => merge_adjacent_text(&mut node.children),
            Inline::Underline(node) => merge_adjacent_text(&mut node.children),
            Inline::Insert(node) => merge_adjacent_text(&mut node.children),
            Inline::Mark(node) => merge_adjacent_text(&mut node.children),
            Inline::Spoiler(node) => merge_adjacent_text(&mut node.children),
            Inline::Subscript(node) => merge_adjacent_text(&mut node.children),
            Inline::Superscript(node) => merge_adjacent_text(&mut node.children),
            _ => {}
        }
    }
}

/// Index into `openers_bottom` for a nested closer's (marker, both-flags,
/// length % 3) key.
fn openers_bottom_key(closer: &DelimMarker) -> usize {
    let marker = match closer.marker {
        b'_' => 1,
        b'~' => 2,
        b'+' => 3,
        b'=' => 4,
        _ => 0,
    };
    let both = usize::from(closer.can_open && closer.can_close);
    // The key holds what `emphasis_delimiters_match` reads of the closer: the
    // whole run's length for the rule of three on `*` / `_`, and what is left
    // of the run for the other marks.
    let length = match closer.marker {
        b'*' | b'_' => closer.run_length,
        _ => closer.length,
    };
    let modulo = length % 3;
    ((marker * 2) + both) * 3 + modulo
}

/// Nested opener/closer compatibility, including CommonMark's rule of three.
fn emphasis_delimiters_match(opener: &DelimMarker, closer: &DelimMarker) -> bool {
    match opener.marker {
        // GFM strikethrough: opener and closer runs must be the same length (a
        // `~` never pairs with `~~`). The rule of three does not apply to `~`.
        b'~' => opener.length == closer.length,
        // `++` / `==` pair two characters from each run.
        b'+' | b'=' => opener.length >= 2 && closer.length >= 2,
        _ => {
            // Rule of three: if either delimiter can both open and close, the
            // sum of the lengths of the runs containing them must not be a
            // multiple of three, unless both are themselves multiples of three.
            // CommonMark counts whole runs, not what earlier pairings left.
            let opener_both = opener.can_open && opener.can_close;
            let closer_both = closer.can_open && closer.can_close;
            if opener_both || closer_both {
                let (opener_run, closer_run) = (opener.run_length, closer.run_length);
                let sum = opener_run + closer_run;
                if sum % 3 == 0 && !(opener_run % 3 == 0 && closer_run % 3 == 0) {
                    return false;
                }
            }
            true
        }
    }
}

/// The node a matched delimiter pair collapses into.
#[derive(Clone, Copy)]
enum EmphasisWrap {
    Emphasis,
    Strong,
    Delete(DeleteMarker),
    Underline,
    Insert,
    Mark,
    Spoiler,
    Subscript,
    Superscript,
}

impl EmphasisWrap {
    /// Whether the node counts toward `MAX_INLINE_NESTING` rather than
    /// `MAX_EMPHASIS_NESTING`.
    fn is_mark(self) -> bool {
        !matches!(self, Self::Emphasis | Self::Strong | Self::Delete(_))
    }
}

/// Wraps the nodes between two delimiter runs into the node `wrap` names and
/// consumes `used` characters from each side.
fn apply_emphasis(
    nodes: &mut InlineList,
    delimiters: &mut [DelimMarker],
    opener_idx: usize,
    closer_idx: usize,
    used: usize,
    wrap: EmphasisWrap,
) {
    let opener_node = delimiters[opener_idx].node_index;
    let closer_node = delimiters[closer_idx].node_index;

    // A span covers exactly its consumed delimiters and what they enclose: the
    // last `used` characters left in the opener's text node through the first
    // `used` left in the closer's.
    let span_start = nodes
        .node_mut(opener_node)
        .span()
        .map(|span| span.end - used);
    let span_end = nodes
        .node_mut(closer_node)
        .span()
        .map(|span| span.start + used);
    // Trim the consumed characters from the opener's text node (right side) and
    // the closer's text node (left side), updating their recorded lengths/spans.
    trim_delimiter_text_tail(nodes.node_mut(opener_node), used);
    trim_delimiter_text_head(nodes.node_mut(closer_node), used);
    delimiters[opener_idx].length -= used;
    let closer = &mut delimiters[closer_idx];
    closer.length -= used;
    closer.position += used;

    // The wrapped children are the nodes strictly between the opener and closer
    // text nodes.
    let meta = NodeMeta::new(
        span_start
            .zip(span_end)
            .map(|(start, end)| Span::new(start, end)),
    );
    nodes.wrap_between(opener_node, closer_node, |children| match wrap {
        EmphasisWrap::Strong => Inline::Strong(Strong { meta, children }),
        EmphasisWrap::Emphasis => Inline::Emphasis(Emphasis { meta, children }),
        EmphasisWrap::Delete(marker) => Inline::Delete(Delete {
            meta,
            marker,
            children,
        }),
        EmphasisWrap::Underline => Inline::Underline(Underline { meta, children }),
        EmphasisWrap::Insert => Inline::Insert(Insert { meta, children }),
        EmphasisWrap::Mark => Inline::Mark(Mark { meta, children }),
        EmphasisWrap::Spoiler => Inline::Spoiler(Spoiler { meta, children }),
        EmphasisWrap::Subscript => Inline::Subscript(Subscript { meta, children }),
        EmphasisWrap::Superscript => Inline::Superscript(Superscript { meta, children }),
    });

    // Drop any placeholder text node that has been fully consumed so leftover
    // delimiters never survive as literal text.
    if delimiters[closer_idx].length == 0 {
        nodes.remove(closer_node);
    }
    if delimiters[opener_idx].length == 0 {
        nodes.remove(opener_node);
    }
}

/// What a `[`-family opener can become when its `]` arrives.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BracketKind {
    Link,
    Image,
    InlineFootnote,
}

/// A `[`, `![`, or `^[` waiting for its `]`. Its `[` is a placeholder text
/// node; an image's `!` and an inline footnote's `^` sit in the node before it.
struct BracketOpener {
    kind: BracketKind,
    /// The placeholder text node holding the `[`.
    node_index: usize,
    /// Offset of the opener's first character (`!`, `^`, or `[`) in the input.
    position: usize,
    /// `delimiters.len()` when the opener was pushed: later runs are in its
    /// label.
    delimiter_bottom: usize,
    /// For an inline footnote, the dormant `^` superscript run recorded before
    /// it, woken if the footnote does not form.
    caret: Option<usize>,
}

/// What a `]` resolves a link-kind opener to.
enum LinkTarget {
    Resource(ParsedLinkResource),
    Reference {
        identifier: String,
        kind: ReferenceKind,
    },
}

/// Matches what follows the `]` at `close` of a link or image whose label is
/// `input[label_start..close]`: an inline `(…)` resource, a full or collapsed
/// reference, or a shortcut reference to a defined label. A `(…)` that is not a
/// valid resource leaves the reference forms to try, for images and links
/// alike. Returns the end of the construct and its target.
fn match_link_target(
    scan: &mut InlineScan,
    input: &str,
    label_start: usize,
    close: usize,
    definitions: Option<Definitions<'_>>,
) -> Option<(usize, LinkTarget)> {
    let label = &input[label_start..close];
    let after = close + 1;
    if input.as_bytes().get(after) == Some(&b'(') {
        match parse_link_resource(&mut scan.lookups, input, after) {
            Some((end, resource)) => return Some((end, LinkTarget::Resource(resource))),
            // A present-but-invalid `(...)` resource is not an inline link or
            // image, but CommonMark still resolves `[label]` as a shortcut
            // reference and leaves the invalid `(...)` as literal text (links
            // 568).
            None => {}
        }
    }
    // A `[` that opens no reference label leaves `[label]` a shortcut
    // reference followed by literal text.
    let reference_close = if input.as_bytes().get(after) == Some(&b'[') {
        scan.reference_label_end(after)
    } else {
        None
    };
    if let Some(reference_close) = reference_close {
        let reference = &input[after + 1..reference_close];
        let identifier = if reference.is_empty() {
            label
        } else {
            reference
        };
        // A present `[...]` second label that resolves to no definition is
        // not a link, and CommonMark does not fall back to treating the first
        // label as a shortcut (`[x][ ]`, `[x][undef]` stay literal).
        return definition_exists(definitions, identifier).then(|| {
            let kind = if reference.is_empty() {
                ReferenceKind::Collapsed
            } else {
                ReferenceKind::Full
            };
            (
                reference_close + 1,
                LinkTarget::Reference {
                    identifier: identifier.into(),
                    kind,
                },
            )
        });
    }
    definition_exists(definitions, label).then(|| {
        (
            after,
            LinkTarget::Reference {
                identifier: label.into(),
                kind: ReferenceKind::Shortcut,
            },
        )
    })
}

/// Builds the link or image node for a matched target over `children`.
fn link_node(target: LinkTarget, image: bool, span: Span, mut children: Vec<Inline>) -> Inline {
    let meta = NodeMeta::new(Some(span));
    if !image {
        demote_links(&mut children);
    }
    match (target, image) {
        (LinkTarget::Resource(resource), false) => Inline::Link(Link {
            meta,
            destination: resource.destination,
            destination_kind: resource.destination_kind,
            title: resource.title,
            title_kind: resource.title_kind,
            children,
        }),
        (LinkTarget::Resource(resource), true) => Inline::Image(Image {
            meta,
            destination: resource.destination,
            destination_kind: resource.destination_kind,
            title: resource.title,
            title_kind: resource.title_kind,
            alt: children,
        }),
        (LinkTarget::Reference { identifier, kind }, false) => {
            Inline::LinkReference(LinkReference {
                meta,
                identifier: normalize_label(&identifier),
                label: identifier,
                kind,
                children,
            })
        }
        (LinkTarget::Reference { identifier, kind }, true) => {
            Inline::ImageReference(ImageReference {
                meta,
                identifier: normalize_label(&identifier),
                label: identifier,
                kind,
                alt: children,
            })
        }
    }
}

/// An autolink: a `Link` spanning `span` to `destination`, whose one child is
/// the URL as written, `text`, spanning `text_span`.
fn autolink_node(span: Span, text_span: Span, destination: String, text: &str) -> Inline {
    Inline::Link(Link {
        meta: NodeMeta::new(Some(span)),
        destination,
        destination_kind: LinkDestinationKind::Bare,
        title: None,
        title_kind: None,
        children: vec![Inline::Text(Text {
            meta: NodeMeta::new(Some(text_span)),
            value: text.into(),
        })],
    })
}

/// Link text holds no links: autolinks inside it, the only links a formed
/// link's text can hold, read as their text, an angle-bracket autolink
/// without its brackets. Image alt text inside it keeps its own links.
fn demote_links(nodes: &mut Vec<Inline>) {
    for node in nodes.iter_mut() {
        match node {
            Inline::Link(link) => {
                let value = link
                    .children
                    .iter()
                    .map(|child| match child {
                        Inline::Text(text) => text.value.as_str(),
                        _ => "",
                    })
                    .collect::<String>();
                *node = Inline::Text(Text {
                    meta: link.meta.clone(),
                    value,
                });
            }
            Inline::Emphasis(node) => demote_links(&mut node.children),
            Inline::Strong(node) => demote_links(&mut node.children),
            Inline::Underline(node) => demote_links(&mut node.children),
            Inline::Delete(node) => demote_links(&mut node.children),
            Inline::Insert(node) => demote_links(&mut node.children),
            Inline::Mark(node) => demote_links(&mut node.children),
            Inline::Spoiler(node) => demote_links(&mut node.children),
            Inline::Subscript(node) => demote_links(&mut node.children),
            Inline::Superscript(node) => demote_links(&mut node.children),
            Inline::InlineFootnote(node) => demote_links(&mut node.children),
            Inline::TextDirective(node) => demote_links(&mut node.label),
            _ => {}
        }
    }
    merge_adjacent_text(nodes);
}

/// The open brackets of one inline pass and the runs and nodes they enclose.
struct Brackets {
    openers: Vec<BracketOpener>,
    /// Openers below this stack index can no longer form links: a link formed
    /// after them.
    link_floor: usize,
    /// How many `[` were left as text for want of nesting room and are still
    /// open: the next that many `]` close them, as text.
    overflow: usize,
    /// Formed link, image, footnote, and directive nodes with their nesting
    /// depth, by node index, so spans around them count it.
    formed: Vec<(usize, usize)>,
}

impl Brackets {
    fn new() -> Self {
        Self {
            openers: Vec::new(),
            link_floor: 0,
            overflow: 0,
            formed: Vec::new(),
        }
    }

    /// Keeps every open `[` from forming a link around what follows.
    fn close_links(&mut self) {
        self.link_floor = self.openers.len();
    }
}

/// Takes the nodes and runs after `opener` out of the pass, pairs the runs
/// among themselves, and returns the resolved label content with its nesting
/// depth.
fn take_label(
    input: &str,
    options: &SyntaxOptions,
    nodes: &mut Vec<Inline>,
    delimiters: &mut Vec<DelimMarker>,
    brackets: &mut Brackets,
    opener: &BracketOpener,
    depth: usize,
) -> (Vec<Inline>, usize) {
    let first = opener.node_index + 1;
    let children: Vec<Inline> = nodes.drain(first..).collect();
    let mut runs = delimiters.split_off(opener.delimiter_bottom);
    for run in &mut runs {
        run.node_index -= first;
    }
    let split = brackets.formed.partition_point(|&(node, _)| node < first);
    let formed: Vec<(usize, usize)> = brackets
        .formed
        .split_off(split)
        .into_iter()
        .map(|(node, marks)| (node - first, marks))
        .collect();
    assign_emphasis_roles(
        input,
        &mut runs,
        options.constructs.gfm_strikethrough,
        options.constructs.underline,
    );
    process_emphasis(
        children,
        runs,
        EmphasisContext {
            underline: options.constructs.underline,
            depth,
        },
        &formed,
    )
}

/// Removes `count` trailing delimiter characters from a placeholder text node.
fn trim_delimiter_text_tail(node: &mut Inline, count: usize) {
    if let Inline::Text(text) = node {
        let new_len = text.value.len().saturating_sub(count);
        text.value.truncate(new_len);
        if let Some(span) = text.meta.span.as_mut() {
            span.end = span.end.saturating_sub(count);
        }
    }
}

/// Removes `count` leading delimiter characters from a placeholder text node.
/// The node holds one repeated marker byte, so dropping from the front leaves
/// the same text as truncating the back.
fn trim_delimiter_text_head(node: &mut Inline, count: usize) {
    if let Inline::Text(text) = node {
        let count = count.min(text.value.len());
        text.value.truncate(text.value.len() - count);
        if let Some(span) = text.meta.span.as_mut() {
            span.start += count;
        }
    }
}

/// Forward lookups the inline scanners ask of one input. `DirectLookups`
/// answers each with a fresh scan, for callers that ask once; `InlineLookups`
/// memoizes the answers for the many questions one inline pass asks. Both
/// answer identically.
trait Lookups {
    /// The first occurrence of `pattern` that starts at or after `from`.
    fn find(&mut self, pattern: &'static str, from: usize) -> Option<usize>;
    /// The first run of exactly `len` backticks at or after `start`, which is
    /// not inside a backtick run.
    fn code_span_close(&mut self, start: usize, len: usize) -> Option<usize>;
    /// The first unescaped `<` or `>`, or line break, at or after `from`.
    fn angle_destination_stop(&mut self, from: usize) -> Option<usize>;
}

struct DirectLookups<'a> {
    input: &'a str,
}

impl Lookups for DirectLookups<'_> {
    fn find(&mut self, pattern: &'static str, from: usize) -> Option<usize> {
        self.input[from..].find(pattern).map(|offset| from + offset)
    }

    fn code_span_close(&mut self, start: usize, len: usize) -> Option<usize> {
        find_code_span_close(self.input, start, len)
    }

    fn angle_destination_stop(&mut self, from: usize) -> Option<usize> {
        (from..self.input.len()).find(|index| is_angle_destination_stop(self.input, *index))
    }
}

/// The patterns `InlineLookups::find` keeps position tables for.
const CACHED_PATTERNS: [&str; 7] = [">", "-->", "?>", "]]>", "\"", "'", "`$"];

/// `Lookups` over one inline input, each answered from a table built on first
/// use.
struct InlineLookups<'a> {
    input: &'a str,
    patterns: [Positions; CACHED_PATTERNS.len()],
    backtick_runs: Option<BTreeMap<usize, Vec<usize>>>,
    angle_destination_stops: Positions,
}

impl<'a> InlineLookups<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            patterns: Default::default(),
            backtick_runs: None,
            angle_destination_stops: Positions::default(),
        }
    }
}

impl Lookups for InlineLookups<'_> {
    fn find(&mut self, pattern: &'static str, from: usize) -> Option<usize> {
        let input = self.input;
        let Some(slot) = CACHED_PATTERNS.iter().position(|cached| *cached == pattern) else {
            return DirectLookups { input }.find(pattern, from);
        };
        self.patterns[slot].first_at_or_after(from, || pattern_starts(input, pattern))
    }

    fn code_span_close(&mut self, start: usize, len: usize) -> Option<usize> {
        let input = self.input;
        let runs = self
            .backtick_runs
            .get_or_insert_with(|| backtick_runs_by_length(input));
        let starts = runs.get(&len)?;
        starts
            .get(starts.partition_point(|run| *run < start))
            .copied()
    }

    fn angle_destination_stop(&mut self, from: usize) -> Option<usize> {
        let input = self.input;
        self.angle_destination_stops.first_at_or_after(from, || {
            (0..input.len())
                .filter(|index| is_angle_destination_stop(input, *index))
                .collect()
        })
    }
}

/// Start offsets of every maximal backtick run, grouped by run length.
fn backtick_runs_by_length(input: &str) -> BTreeMap<usize, Vec<usize>> {
    let mut runs: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut index = 0;
    while index < input.len() {
        if input.as_bytes()[index] == b'`' {
            let len = backtick_run_len(input, index);
            runs.entry(len).or_default().push(index);
            index += len;
        } else {
            index += 1;
        }
    }
    runs
}

fn backtick_run_len(input: &str, index: usize) -> usize {
    input.as_bytes()[index..]
        .iter()
        .take_while(|byte| **byte == b'`')
        .count()
}

fn is_angle_destination_stop(input: &str, index: usize) -> bool {
    match input.as_bytes()[index] {
        b'<' | b'>' => !is_escaped_at(input, index),
        b'\n' | b'\r' => true,
        _ => false,
    }
}

/// Everything one `parse_inline_content` pass memoizes about its input, so its
/// questions about closing delimiters cost amortized linear time in total.
struct InlineScan<'a> {
    input: &'a str,
    lookups: InlineLookups<'a>,
    label_ends: BracketMemo,
    reference_label_ends: PathMemo,
    wikilink_closes: PathMemo,
    directive_attribute_closes: PathMemo,
    mdx_expression_closes: BracketMemo,
    jsx: Option<JsxIndex>,
    single_tilde_delete_closes: Positions,
    literal_autolinks: LiteralAutolinkScan,
}

impl<'a> InlineScan<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            lookups: InlineLookups::new(input),
            label_ends: BracketMemo::default(),
            reference_label_ends: PathMemo::default(),
            wikilink_closes: PathMemo::default(),
            directive_attribute_closes: PathMemo::default(),
            mdx_expression_closes: BracketMemo::default(),
            jsx: None,
            single_tilde_delete_closes: Positions::default(),
            literal_autolinks: LiteralAutolinkScan::default(),
        }
    }

    /// `find_link_label_end`, memoized.
    fn link_label_end(&mut self, open: usize) -> Option<usize> {
        let input = self.input;
        if input.as_bytes().get(open) != Some(&b'[') {
            return None;
        }
        let lookups = &mut self.lookups;
        self.label_ends
            .resolve(input.len() + 1, open + 1, |cursor| {
                link_label_step(lookups, input, cursor)
            })
    }

    /// `find_reference_label_end`, memoized.
    fn reference_label_end(&mut self, open: usize) -> Option<usize> {
        let input = self.input;
        if input.as_bytes().get(open) != Some(&b'[') {
            return None;
        }
        let close = self
            .reference_label_ends
            .resolve(input.len() + 1, open + 1, |cursor| {
                reference_label_step(input, cursor)
            })?;
        reference_label_is_within_limit(&input[open + 1..close]).then_some(close)
    }

    /// The `]]` closing the wikilink whose content starts at `start`; it must
    /// be on the same line.
    fn wikilink_close(&mut self, start: usize) -> Option<usize> {
        let input = self.input;
        self.wikilink_closes
            .resolve(input.len() + 1, start, |cursor| {
                wikilink_close_step(input, cursor)
            })
    }

    /// The `}` closing the inline MDX expression opening at `open`.
    fn mdx_expression_close(&mut self, open: usize) -> Option<usize> {
        let input = self.input;
        if input.as_bytes().get(open) != Some(&b'{') {
            return None;
        }
        self.mdx_expression_closes
            .resolve(
                (input.len() + 1) * MDX_BRACE_STATES,
                (open + 1) * MDX_BRACE_STATES,
                |node| mdx_expression_step(input, node, false),
            )
            .map(|node| node / MDX_BRACE_STATES)
    }

    /// The end of the inline MDX JSX element opening at `start`.
    fn mdx_jsx_end(&mut self, start: usize) -> Option<usize> {
        let input = self.input;
        let expressions = &mut self.mdx_expression_closes;
        self.jsx
            .get_or_insert_with(|| JsxIndex::new(input, false, expressions))
            .element_end(start)
            .map(|end| end + 1)
    }

    /// The first `~` at or after `start` that can close a single-tilde delete.
    fn single_tilde_delete_close(&mut self, start: usize) -> Option<usize> {
        let input = self.input;
        self.single_tilde_delete_closes
            .first_at_or_after(start, || {
                (0..input.len())
                    .filter(|index| is_single_tilde_delete_close(input, *index))
                    .collect()
            })
    }

    /// `find_directive_attributes_close`, memoized.
    fn directive_attributes_close(&mut self, open: usize) -> Option<usize> {
        let input = self.input;
        if input.as_bytes().get(open) != Some(&b'{') {
            return None;
        }
        self.directive_attribute_closes.resolve(
            (input.len() + 1) * DIRECTIVE_QUOTE_STATES,
            (open + 1) * DIRECTIVE_QUOTE_STATES,
            |node| directive_attributes_step(input, node),
        )
    }
}

/// Parses the inline content `input`, whose source map is `map`. The inline
/// parser works in `input`'s coordinates; the spans it produces, on nodes and
/// on the diagnostics it pushes, are translated to the input afterwards.
fn parse_inlines(
    input: &str,
    map: &SourceMap,
    options: &SyntaxOptions,
    definitions: Option<Definitions<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Inline> {
    parse_inlines_in(
        input,
        map,
        InlineState::default(),
        options,
        definitions,
        diagnostics,
    )
}

/// The inline content of a table cell, whose input reads each `\|` as `|`:
/// `escaped_pipes` holds the offset of each such `|`, which is an `Escape`
/// where it stands in text.
fn parse_cell_inlines(
    input: &str,
    map: &SourceMap,
    escaped_pipes: Vec<usize>,
    options: &SyntaxOptions,
    definitions: Option<Definitions<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Inline> {
    let state = InlineState {
        escaped_pipes,
        ..InlineState::default()
    };
    parse_inlines_in(input, map, state, options, definitions, diagnostics)
}

fn parse_inlines_in(
    input: &str,
    map: &SourceMap,
    mut state: InlineState,
    options: &SyntaxOptions,
    definitions: Option<Definitions<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Inline> {
    // Definitions are collected from the block structure alone.
    if definitions.is_none() {
        return Vec::new();
    }
    let first_diagnostic = diagnostics.len();
    let mut nodes =
        parse_inlines_with_context(input, 0, options, definitions, diagnostics, &mut state);
    source_map::translate_inlines(map, &mut nodes, &mut diagnostics[first_diagnostic..]);
    nodes
}

/// The deepest inline nesting the parser builds. Directive labels parse their
/// content one inline pass deeper; within a pass, open brackets and every
/// emphasis or mark span each add a level. Content past this depth stays
/// literal text, so recursion and the native stack it uses stay bounded.
const MAX_INLINE_NESTING: usize = 32;

/// State shared by every inline parse nested under one block-level inline
/// parse. All nested inputs are slices of that one input, addressed by their
/// start offset in it; `parse_inlines` maps the result to the original input.
#[derive(Default)]
struct InlineState {
    /// How many inline parses enclose the current one.
    depth: usize,
    /// In a table cell, the sorted offsets of the `|`s the cell's input reads
    /// from `\|`.
    escaped_pipes: Vec<usize>,
}

impl InlineState {
    fn is_escaped_pipe(&self, offset: usize) -> bool {
        !self.escaped_pipes.is_empty() && self.escaped_pipes.binary_search(&offset).is_ok()
    }
}

/// What one inline pass threads through its construct parsers: the state it
/// shares with enclosing passes and the memoized scans of its own input.
struct InlinePass<'p, 'a> {
    state: &'p mut InlineState,
    scan: InlineScan<'a>,
}

fn parse_inlines_with_context(
    input: &str,
    base_offset: usize,
    options: &SyntaxOptions,
    definitions: Option<Definitions<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
    state: &mut InlineState,
) -> Vec<Inline> {
    if state.depth >= MAX_INLINE_NESTING {
        if input.is_empty() {
            return Vec::new();
        }
        return vec![Inline::Text(Text {
            meta: NodeMeta::new(Some(Span::new(base_offset, base_offset + input.len()))),
            value: input.into(),
        })];
    }
    state.depth += 1;
    let nodes = parse_inline_content(input, base_offset, options, definitions, diagnostics, state);
    state.depth -= 1;
    nodes
}

fn parse_inline_content(
    input: &str,
    base_offset: usize,
    options: &SyntaxOptions,
    definitions: Option<Definitions<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
    state: &mut InlineState,
) -> Vec<Inline> {
    let bytes = input.as_bytes();
    let mut nodes = Vec::new();
    let mut text_start = 0;
    let mut text = String::new();
    let mut index = 0;
    // Emphasis and mark delimiters are resolved with a delimiter stack after the
    // scan completes. During the scan we emit each candidate delimiter run as a
    // literal text node and record it here so `process_emphasis` can rewrite the
    // flat node list into emphasis and mark nodes (or leave it as text).
    let mut delimiters: Vec<DelimMarker> = Vec::new();
    let mut lines = LineCounter::default();
    let mut brackets = Brackets::new();
    // The run and offset of the `^` a superscript is waiting to close.
    let mut superscript_open: Option<(usize, usize)> = None;
    let mut pass = InlinePass {
        state,
        scan: InlineScan::new(input),
    };
    // A literal autolink holds `://`, `www.`, or an email's `@`; content
    // without any of them is not scanned for one at each position.
    let literal_autolinks = (options.constructs.gfm_autolink_literal
        || options.constructs.relaxed_autolinks)
        && (bytes.contains(&b'@')
            || input.contains("://")
            || bytes
                .windows(4)
                .any(|window| window.eq_ignore_ascii_case(b"www.")));

    while index < bytes.len() {
        if bytes[index] == b'\\' {
            if let Some((next_index, char)) = next_char(input, index + 1) {
                if char.is_ascii_punctuation() {
                    flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                    nodes.push(Inline::Escape(Escape {
                        meta: NodeMeta::new(Some(Span::new(
                            base_offset + index,
                            base_offset + next_index,
                        ))),
                        value: char,
                    }));
                    index = next_index;
                    text_start = index;
                    continue;
                }
            }
        }

        if bytes[index] == b'&' {
            if let Some((end, value)) = parse_character_reference(input, index) {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                nodes.push(Inline::CharacterReference(CharacterReference {
                    meta: NodeMeta::new(Some(Span::new(base_offset + index, base_offset + end))),
                    reference: input[index..end].into(),
                    value,
                }));
                index = end;
                text_start = index;
                continue;
            }
        }

        // A cell's `|` read from `\|` is an escape, never a spoiler bar.
        if bytes[index] == b'|' && pass.state.is_escaped_pipe(base_offset + index) {
            flush_text(&mut nodes, &mut text, text_start, base_offset + index);
            nodes.push(Inline::Escape(Escape {
                meta: NodeMeta::new(Some(Span::new(
                    base_offset + index,
                    base_offset + index + 1,
                ))),
                value: '|',
            }));
            index += 1;
            text_start = index;
            continue;
        }

        if bytes[index] == b'\n' {
            // Only a literal backslash, not one an escape or a reference wrote,
            // makes the line ending a hard break.
            if index > 0
                && bytes[index - 1] == b'\\'
                && !is_escaped_at(input, index - 1)
                && text.ends_with('\\')
            {
                text.pop();
                flush_text(
                    &mut nodes,
                    &mut text,
                    text_start,
                    base_offset + index.saturating_sub(1),
                );
                nodes.push(Inline::LineBreak(LineBreak {
                    meta: NodeMeta::new(Some(Span::new(
                        base_offset + index.saturating_sub(1),
                        base_offset + index + 1,
                    ))),
                    kind: LineBreakKind::Backslash,
                }));
                index += 1;
                text_start = index;
                continue;
            }
            // The spaces and tabs ending the line: a hard break or stripped.
            let trailing_spaces = trailing_space_count(&text);
            if is_hard_break_suffix(&text, trailing_spaces) {
                text.truncate(text.len() - trailing_spaces);
                flush_text(
                    &mut nodes,
                    &mut text,
                    text_start,
                    base_offset + index.saturating_sub(trailing_spaces),
                );
                nodes.push(Inline::LineBreak(LineBreak {
                    meta: NodeMeta::new(Some(Span::new(
                        base_offset + index.saturating_sub(trailing_spaces),
                        base_offset + index + 1,
                    ))),
                    kind: LineBreakKind::Spaces,
                }));
                index += 1;
                text_start = index;
                continue;
            }
            if trailing_spaces > 0 {
                text.truncate(text.len() - trailing_spaces);
            }
            flush_text(&mut nodes, &mut text, text_start, base_offset + index);
            nodes.push(Inline::SoftBreak(SoftBreak {
                meta: NodeMeta::new(Some(Span::new(
                    base_offset + index,
                    base_offset + index + 1,
                ))),
            }));
            index += 1;
            text_start = index;
            continue;
        }

        if bytes[index] == b'`' {
            if let Some((end, code_span)) =
                parse_code_span_with(&mut pass.scan.lookups, input, index)
            {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                nodes.push(Inline::Code(CodeInline {
                    meta: NodeMeta::new(Some(Span::new(base_offset + index, base_offset + end))),
                    value: code_span.value,
                    raw: code_span.raw,
                    fence_length: code_span.fence_length,
                }));
                index = end;
                text_start = index;
                continue;
            } else {
                // No matching-length close for this opening backtick run:
                // CommonMark renders the whole run as literal text. Consume the
                // entire run here so the loop does not advance one byte and retry
                // a shorter sub-run that could spuriously match a shorter close
                // (```foo`` stayed a phantom 2-backtick code span).
                let run = bytes[index..]
                    .iter()
                    .take_while(|byte| **byte == b'`')
                    .count();
                if text.is_empty() {
                    text_start = base_offset + index;
                }
                for _ in 0..run {
                    text.push('`');
                }
                index += run;
                continue;
            }
        }

        if options.constructs.spoiler && bytes[index] == b'|' {
            // A cell's escaped pipe ends the run and is no bar before it.
            let bars = delimiter_byte_run_len(input, index, b'|');
            let run_len = (index..index + bars)
                .position(|at| pass.state.is_escaped_pipe(base_offset + at))
                .unwrap_or(bars);
            if run_len >= 2 {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                // A spoiler run opens with its last two bars and closes with
                // its first two, unless a bar precedes it.
                let roles = DelimRoles {
                    can_open: true,
                    can_close: index == 0
                        || bytes[index - 1] != b'|'
                        || pass.state.is_escaped_pipe(base_offset + index - 1),
                    ..DelimRoles::default()
                };
                push_delimiter(
                    &mut nodes,
                    &mut delimiters,
                    &mut lines,
                    input,
                    index,
                    base_offset,
                    b'|',
                    run_len,
                    roles,
                );
                index += run_len;
                text_start = index;
                continue;
            }
        }

        if bytes[index] == b'*' && delimiter_byte_run_start(input, index, b'*') == index {
            let run_len = delimiter_byte_run_len(input, index, b'*');
            flush_text(&mut nodes, &mut text, text_start, base_offset + index);
            // Roles are settled by `assign_emphasis_roles`.
            let roles = DelimRoles::default();
            push_delimiter(
                &mut nodes,
                &mut delimiters,
                &mut lines,
                input,
                index,
                base_offset,
                b'*',
                run_len,
                roles,
            );
            index += run_len;
            text_start = index;
            continue;
        }

        // Core `_` emphasis/strong is resolved by the delimiter stack, just like
        // `*`; with underline enabled, a `__` pair forms `Underline` instead of
        // `Strong`.
        if bytes[index] == b'_' && delimiter_byte_run_start(input, index, b'_') == index {
            // A leading `_` can begin a GFM email local part (`_a@b.c`); try the
            // literal autolink before recording the `_` as an emphasis
            // delimiter, otherwise the `_` would be consumed and the email would
            // wrongly start one char later (where its left boundary fails).
            if literal_autolinks {
                if let Some((end, destination)) = parse_literal_autolink(
                    input,
                    index,
                    options.constructs.gfm_autolink_literal,
                    options.constructs.relaxed_autolinks,
                    wikilinks_enabled(options),
                    &mut pass.scan.literal_autolinks,
                ) {
                    flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                    let span = Span::new(base_offset + index, base_offset + end);
                    nodes.push(autolink_node(span, span, destination, &input[index..end]));
                    index = end;
                    text_start = index;
                    continue;
                }
            }
            let run_len = delimiter_byte_run_len(input, index, b'_');
            flush_text(&mut nodes, &mut text, text_start, base_offset + index);
            // Roles are settled by `assign_emphasis_roles`.
            let roles = DelimRoles::default();
            push_delimiter(
                &mut nodes,
                &mut delimiters,
                &mut lines,
                input,
                index,
                base_offset,
                b'_',
                run_len,
                roles,
            );
            index += run_len;
            text_start = index;
            continue;
        }

        if (options.constructs.insert && bytes[index] == b'+')
            || (options.constructs.highlight && bytes[index] == b'=')
        {
            let marker = bytes[index];
            let run_len = delimiter_byte_run_len(input, index, marker);
            if run_len >= 2 {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                let roles = double_mark_roles(input, index, run_len, marker);
                push_delimiter(
                    &mut nodes,
                    &mut delimiters,
                    &mut lines,
                    input,
                    index,
                    base_offset,
                    marker,
                    run_len,
                    roles,
                );
                index += run_len;
                text_start = index;
                continue;
            }
        }

        let bracket_room = pass.state.depth - 1 + brackets.openers.len() < MAX_INLINE_NESTING;

        // A `^` that a superscript on its line is waiting for closes it, even
        // before a `[`.
        let closes_superscript = options.constructs.superscript
            && bytes[index] == b'^'
            && superscript_open.is_some_and(|(run, position)| {
                // A run taken into or dropped with a bracket label is gone.
                delimiters.get(run).is_some_and(|run| {
                    run.position == position && run.line == lines.line_at(input, index)
                }) && position + 1 != index
            });

        // `^[` opens an inline footnote. Its `^` is recorded as a dormant
        // superscript run, woken if the footnote does not form.
        if options.constructs.inline_footnote
            && options.constructs.footnote_reference
            && bracket_room
            && !closes_superscript
            && bytes[index] == b'^'
            && bytes.get(index + 1) == Some(&b'[')
        {
            flush_text(&mut nodes, &mut text, text_start, base_offset + index);
            let caret = if options.constructs.superscript {
                push_delimiter(
                    &mut nodes,
                    &mut delimiters,
                    &mut lines,
                    input,
                    index,
                    base_offset,
                    b'^',
                    1,
                    DelimRoles::default(),
                );
                Some(delimiters.len() - 1)
            } else {
                push_text(&mut nodes, base_offset + index, "^");
                None
            };
            push_text(&mut nodes, base_offset + index + 1, "[");
            brackets.openers.push(BracketOpener {
                kind: BracketKind::InlineFootnote,
                node_index: nodes.len() - 1,
                position: index,
                delimiter_bottom: delimiters.len(),
                caret,
            });
            index += 2;
            text_start = index;
            continue;
        }

        // A `^` opens a superscript unless it starts an inline footnote, and
        // closes one at the next `^` on its line.
        if options.constructs.superscript && bytes[index] == b'^' {
            flush_text(&mut nodes, &mut text, text_start, base_offset + index);
            let roles = DelimRoles {
                single_open: !(options.constructs.inline_footnote
                    && bytes.get(index + 1) == Some(&b'[')),
                single_close: true,
                ..DelimRoles::default()
            };
            push_delimiter(
                &mut nodes,
                &mut delimiters,
                &mut lines,
                input,
                index,
                base_offset,
                b'^',
                1,
                roles,
            );
            // Track the superscript this run leaves open, as the stack will
            // pair it, so a later `^[` knows whether its `^` closes one.
            superscript_open = if !closes_superscript && roles.single_open {
                Some((delimiters.len() - 1, index))
            } else {
                None
            };
            index += 1;
            text_start = index;
            continue;
        }

        // A `~` run is recorded once for both of its possible roles: GFM
        // strikethrough pairs runs of equal length (2, or 1 in single-tilde
        // mode) like `*`/`_`; a subscript opens with a lone `~` and closes at
        // the first `~` that follows on its line.
        if (options.constructs.gfm_strikethrough || options.constructs.subscript)
            && bytes[index] == b'~'
        {
            let run_len = delimiter_byte_run_len(input, index, b'~');
            let mut roles = DelimRoles::default();
            if options.constructs.gfm_strikethrough
                && delimiter_byte_run_start(input, index, b'~') == index
                && (run_len == 2 || (run_len == 1 && options.parse.single_tilde_strikethrough))
            {
                roles.strike = true;
            }
            if options.constructs.subscript {
                roles.single_open = starts_exact_byte_run(input, index, b'~', 1)
                    && !single_tilde_delete_takes_precedence(options, input, index, &mut pass.scan);
                roles.single_close = true;
            }
            if roles.strike || roles.single_open || roles.single_close {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                push_delimiter(
                    &mut nodes,
                    &mut delimiters,
                    &mut lines,
                    input,
                    index,
                    base_offset,
                    b'~',
                    run_len,
                    roles,
                );
                index += run_len;
                text_start = index;
                continue;
            }
        }

        if bytes[index] == b'!' && bytes.get(index + 1) == Some(&b'[') {
            // A wikilink at the `[` wins over the image, as it wins over a
            // link at a lone `[`, and the `!` makes it an embed; it takes no
            // bracket nesting, so it forms past the limit too.
            if let Some((end, wikilink)) =
                parse_wikilink(input, index + 1, base_offset, options, &mut pass.scan)
            {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                nodes.push(embed(wikilink, base_offset + index));
                index = end;
                text_start = index;
                continue;
            }
        }

        if bytes[index] == b'!' && bytes.get(index + 1) == Some(&b'[') && bracket_room {
            flush_text(&mut nodes, &mut text, text_start, base_offset + index);
            push_text(&mut nodes, base_offset + index, "!");
            push_text(&mut nodes, base_offset + index + 1, "[");
            brackets.openers.push(BracketOpener {
                kind: BracketKind::Image,
                node_index: nodes.len() - 1,
                position: index,
                delimiter_bottom: delimiters.len(),
                caret: None,
            });
            index += 2;
            text_start = index;
            continue;
        }

        if bytes[index] == b'[' {
            if let Some((end, wikilink)) =
                parse_wikilink(input, index, base_offset, options, &mut pass.scan)
            {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                nodes.push(wikilink);
                index = end;
                text_start = index;
                continue;
            }
            if bracket_room {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                push_text(&mut nodes, base_offset + index, "[");
                brackets.openers.push(BracketOpener {
                    kind: BracketKind::Link,
                    node_index: nodes.len() - 1,
                    position: index,
                    delimiter_bottom: delimiters.len(),
                    caret: None,
                });
                index += 1;
                text_start = index;
                continue;
            }
            brackets.overflow += 1;
        }

        if bytes[index] == b']' && brackets.overflow > 0 {
            // Closes a `[` left as text past the nesting limit.
            brackets.overflow -= 1;
        } else if bytes[index] == b']' && !brackets.openers.is_empty() {
            flush_text(&mut nodes, &mut text, text_start, base_offset + index);
            if let Some(end) = close_bracket(
                input,
                base_offset,
                index,
                options,
                definitions,
                &mut nodes,
                &mut delimiters,
                &mut brackets,
                &mut pass,
            ) {
                index = end;
                text_start = index;
                continue;
            }
        }

        if bytes[index] == b'$' && options.constructs.math_inline {
            if let Some((end, value, kind)) =
                parse_math_inline(&mut pass.scan.lookups, input, index)
            {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                nodes.push(Inline::Math(MathInline {
                    meta: NodeMeta::new(Some(Span::new(base_offset + index, base_offset + end))),
                    value,
                    kind,
                }));
                index = end;
                text_start = index;
                continue;
            }
            // A dollar run that opens but finds no exact-length close is emitted
            // as literal text in one piece (like a code-span). Skipping the
            // whole run prevents re-opening with a shorter marker inside it, so
            // `$$$foo$$` stays literal rather than matching `$$foo$$`. A lone
            // `$` before a backtick (the code-math form) is a run of 1, so this
            // still advances correctly when that form fails.
            let run = bytes[index..]
                .iter()
                .take_while(|byte| **byte == b'$')
                .count();
            if run > 1 {
                if text.is_empty() {
                    text_start = base_offset + index;
                }
                text.push_str(&input[index..index + run]);
                index += run;
                continue;
            }
        }

        // A bare URL is an autolink even inside an open bracket; if the bracket
        // forms a link, `demote_links` turns it back into text.
        if literal_autolinks {
            if let Some((end, destination)) = parse_literal_autolink(
                input,
                index,
                options.constructs.gfm_autolink_literal,
                options.constructs.relaxed_autolinks,
                wikilinks_enabled(options),
                &mut pass.scan.literal_autolinks,
            ) {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                let span = Span::new(base_offset + index, base_offset + end);
                nodes.push(autolink_node(span, span, destination, &input[index..end]));
                index = end;
                text_start = index;
                continue;
            }
        }

        if bytes[index] == b'<' {
            if let Some(end) = pass.scan.lookups.find(">", index).map(|close| close + 1) {
                let uri = &input[index + 1..end - 1];
                if let Some(destination) = angle_autolink_destination(uri) {
                    flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                    nodes.push(autolink_node(
                        Span::new(base_offset + index, base_offset + end),
                        Span::new(base_offset + index + 1, base_offset + end - 1),
                        destination,
                        uri,
                    ));
                    index = end;
                    text_start = index;
                    continue;
                }
            }
            if options.constructs.mdx_jsx_inline {
                if let Some((end, raw)) = pass
                    .scan
                    .mdx_jsx_end(index)
                    .map(|end| (end, String::from(&input[index..end])))
                {
                    flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                    nodes.push(Inline::MdxJsx(MdxJsxInline {
                        meta: NodeMeta::new(Some(Span::new(
                            base_offset + index,
                            base_offset + end,
                        ))),
                        value: raw,
                    }));
                    index = end;
                    text_start = index;
                    continue;
                }
            }
            if options.constructs.html_inline {
                if let Some(end) = html_inline_end(&mut pass.scan.lookups, input, index) {
                    flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                    nodes.push(Inline::Html(HtmlInline {
                        meta: NodeMeta::new(Some(Span::new(
                            base_offset + index,
                            base_offset + end,
                        ))),
                        value: input[index..end].into(),
                    }));
                    index = end;
                    text_start = index;
                    continue;
                }
            }
        }

        if bytes[index] == b'{' && options.constructs.mdx_expression_inline {
            if let Some(end) = pass.scan.mdx_expression_close(index) {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                nodes.push(Inline::MdxExpression(MdxExpressionInline {
                    meta: NodeMeta::new(Some(Span::new(
                        base_offset + index,
                        base_offset + end + 1,
                    ))),
                    value: input[index + 1..end].into(),
                }));
                index = end + 1;
                text_start = index;
                continue;
            } else {
                diagnostics.push(Diagnostic::new(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::InvalidMdx,
                    Span::new(base_offset + index, base_offset + input.len()),
                    "MDX expression is missing a closing brace",
                ));
            }
        }

        if bytes[index] == b':' && options.constructs.shortcode {
            if let Some((end, name)) = parse_shortcode(input, index) {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                nodes.push(Inline::Shortcode(Shortcode {
                    meta: NodeMeta::new(Some(Span::new(base_offset + index, base_offset + end))),
                    name,
                }));
                index = end;
                text_start = index;
                continue;
            }
        }

        // A directive counts one nesting level for itself, on top of the
        // enclosing passes and open brackets.
        if bytes[index] == b':'
            && options.constructs.directive_text
            && pass.state.depth + brackets.openers.len() <= MAX_INLINE_NESTING
        {
            // The label parses one pass deeper than the open brackets around
            // it.
            pass.state.depth += brackets.openers.len();
            let directive = parse_text_directive(
                input,
                index,
                base_offset,
                options,
                definitions,
                diagnostics,
                &mut pass,
            );
            pass.state.depth -= brackets.openers.len();
            if let Some((end, directive)) = directive {
                flush_text(&mut nodes, &mut text, text_start, base_offset + index);
                if let Inline::TextDirective(node) = &directive {
                    // A link inside the directive label keeps open brackets
                    // from forming links around it.
                    if contains_link_inline(&node.label) {
                        brackets.close_links();
                    }
                    let depth = inline_nesting(&node.label) + 1;
                    brackets.formed.push((nodes.len(), depth));
                }
                nodes.push(directive);
                index = end;
                text_start = index;
                continue;
            }
        }

        let (next_index, char) = next_char(input, index).expect("valid UTF-8 byte index");
        if text.is_empty() {
            text_start = base_offset + index;
        }
        text.push(char);
        index = next_index;
    }

    flush_text(&mut nodes, &mut text, text_start, base_offset + input.len());
    // An inline footnote that never closed leaves its `^` free to close a
    // superscript.
    for opener in &brackets.openers {
        if let Some(caret) = opener.caret {
            delimiters[caret].single_close = true;
        }
    }
    assign_emphasis_roles(
        input,
        &mut delimiters,
        options.constructs.gfm_strikethrough,
        options.constructs.underline,
    );
    process_emphasis(
        nodes,
        delimiters,
        EmphasisContext {
            underline: options.constructs.underline,
            depth: pass.state.depth - 1,
        },
        &brackets.formed,
    )
    .0
}

/// A `:name:` shortcode at `index`: `name` is in the gemoji table, and the
/// source chars directly outside the colons are no Unicode letter or digit,
/// so clock times and words joined by a colon stay text.
fn parse_shortcode(input: &str, index: usize) -> Option<(usize, String)> {
    if input[index..].starts_with("::") {
        return None;
    }
    if input[..index]
        .chars()
        .next_back()
        .is_some_and(char::is_alphanumeric)
    {
        return None;
    }

    let mut cursor = index + 1;
    while let Some((next, char)) = next_char(input, cursor) {
        if char == ':' {
            let name = &input[index + 1..cursor];
            if crate::gemoji::glyph(name).is_none()
                || input[next..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric)
            {
                return None;
            }
            return Some((next, name.into()));
        }
        if !(char.is_ascii_alphanumeric() || matches!(char, '_' | '-' | '+')) {
            return None;
        }
        cursor = next;
    }
    None
}

fn parse_wikilink(
    input: &str,
    index: usize,
    base_offset: usize,
    options: &SyntaxOptions,
    scan: &mut InlineScan,
) -> Option<(usize, Inline)> {
    let configured_order = if options.constructs.wikilink_title_after_pipe {
        WikiLinkLabelOrder::AfterPipe
    } else if options.constructs.wikilink_title_before_pipe {
        WikiLinkLabelOrder::BeforePipe
    } else {
        return None;
    };
    if input.as_bytes().get(index) != Some(&b'[') || input.as_bytes().get(index + 1) != Some(&b'[')
    {
        return None;
    }

    let close = scan.wikilink_close(index + 2)?;
    let source = &input[index + 2..close];
    if source.is_empty() || source.len() > WIKILINK_MAX_BYTES {
        return None;
    }

    let (target_source, label_source, label_order) =
        if let Some(separator) = find_wikilink_separator(source) {
            match configured_order {
                WikiLinkLabelOrder::AfterPipe => (
                    &source[..separator],
                    &source[separator + 1..],
                    WikiLinkLabelOrder::AfterPipe,
                ),
                WikiLinkLabelOrder::BeforePipe => (
                    &source[separator + 1..],
                    &source[..separator],
                    WikiLinkLabelOrder::BeforePipe,
                ),
            }
        } else {
            (source, source, configured_order)
        };

    let target = unescape_string(target_source);
    if target.is_empty() {
        return None;
    }
    let label = unescape_string(label_source);
    let end = close + 2;
    Some((
        end,
        Inline::WikiLink(WikiLink {
            meta: NodeMeta::new(Some(Span::new(base_offset + index, base_offset + end))),
            target,
            label,
            label_order,
            embed: false,
        }),
    ))
}

/// `wikilink` marked as an embed by the `!` at `bang`, where its span starts.
fn embed(mut wikilink: Inline, bang: usize) -> Inline {
    if let Inline::WikiLink(node) = &mut wikilink {
        node.embed = true;
        node.meta.span = node.meta.span.map(|span| Span::new(bang, span.end));
    }
    wikilink
}

/// One step of the walk to a wikilink's closing `]]`, which must be on the
/// opener's line.
fn wikilink_close_step(input: &str, cursor: usize) -> Step {
    let bytes = input.as_bytes();
    match bytes.get(cursor) {
        None | Some(b'\n' | b'\r') => Step::Done(None),
        Some(b'\\') => {
            let escaped = cursor + 1;
            Step::Next(next_char(input, escaped).map_or(escaped, |(after_escape, _)| after_escape))
        }
        Some(b']') if bytes.get(cursor + 1) == Some(&b']') => Step::Done(Some(cursor)),
        Some(_) => Step::Next(next_char(input, cursor).map_or(input.len(), |(next, _)| next)),
    }
}

fn find_wikilink_separator(input: &str) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut cursor = 0;
    while cursor < input.len() {
        match bytes[cursor] {
            b'\\' => {
                cursor += 1;
                if cursor < input.len() {
                    cursor = next_char(input, cursor)?.0;
                }
            }
            b'|' => return Some(cursor),
            _ => cursor = next_char(input, cursor)?.0,
        }
    }
    None
}

fn trailing_space_count(input: &str) -> usize {
    input
        .as_bytes()
        .iter()
        .rev()
        .take_while(|byte| matches!(**byte, b' ' | b'\t'))
        .count()
}

fn is_hard_break_suffix(input: &str, trailing: usize) -> bool {
    // A hard line break is two or more spaces immediately before the newline
    // with no intervening tab; a tab anywhere in the trailing whitespace run
    // demotes it to a soft break.
    let bytes = input.as_bytes();
    trailing >= 2
        && bytes[bytes.len() - trailing..]
            .iter()
            .all(|byte| *byte == b' ')
}

fn find_reference_label_end(input: &str, open: usize) -> Option<usize> {
    // A reference/definition link label does not nest: it ends at the first
    // unescaped `]`, and an unescaped interior `[` disqualifies it.
    if input.as_bytes().get(open) != Some(&b'[') {
        return None;
    }
    let close = path_walk(open + 1, |cursor| reference_label_step(input, cursor))?;
    reference_label_is_within_limit(&input[open + 1..close]).then_some(close)
}

/// One step of the walk to a reference label's closing `]`.
fn reference_label_step(input: &str, cursor: usize) -> Step {
    let Some((next, char)) = next_char(input, cursor) else {
        return Step::Done(None);
    };
    match char {
        '\\' => Step::Next(next_char(input, next).map_or(next, |(after_escape, _)| after_escape)),
        '[' => Step::Done(None),
        ']' => Step::Done(Some(cursor)),
        _ => Step::Next(next),
    }
}

/// How many container levels `inlines` nest, counting empty containers.
fn inline_nesting(inlines: &[Inline]) -> usize {
    let mut deepest = 0;
    let mut pending = vec![(inlines, 0)];
    while let Some((nodes, depth)) = pending.pop() {
        for node in nodes {
            if is_inline_container(node) {
                deepest = deepest.max(depth + 1);
                pending.push((node.children(), depth + 1));
            }
        }
    }
    deepest
}

fn is_inline_container(inline: &Inline) -> bool {
    matches!(
        inline,
        Inline::Emphasis(_)
            | Inline::Strong(_)
            | Inline::Underline(_)
            | Inline::Delete(_)
            | Inline::Insert(_)
            | Inline::Mark(_)
            | Inline::Subscript(_)
            | Inline::Superscript(_)
            | Inline::Spoiler(_)
            | Inline::Link(_)
            | Inline::Image(_)
            | Inline::LinkReference(_)
            | Inline::ImageReference(_)
            | Inline::InlineFootnote(_)
            | Inline::TextDirective(_)
    )
}

/// Whether `inlines` hold a link formed from brackets, at any depth: an
/// autolink, which keeps open brackets from nothing, does not count.
fn contains_link_inline(inlines: &[Inline]) -> bool {
    inlines.iter().any(|inline| {
        let formed = match inline {
            Inline::Link(link) => !is_autolink(link),
            Inline::LinkReference(_) => true,
            _ => false,
        };
        formed || contains_link_inline(inline.children())
    })
}

/// Whether `link`, as parsed, is an autolink: its one text child spans the
/// whole link, as a literal autolink's does, or all of it but the angle
/// brackets around it.
fn is_autolink(link: &Link) -> bool {
    let [Inline::Text(text)] = link.children.as_slice() else {
        return false;
    };
    let (Some(outer), Some(inner)) = (link.meta.span, text.meta.span) else {
        return false;
    };
    (inner.start == outer.start && inner.end == outer.end)
        || (inner.start == outer.start + 1 && inner.end + 1 == outer.end)
}

fn find_link_label_end(input: &str, open: usize) -> Option<usize> {
    if input.as_bytes().get(open) != Some(&b'[') {
        return None;
    }
    let mut lookups = DirectLookups { input };
    bracket_walk(open + 1, |cursor| {
        link_label_step(&mut lookups, input, cursor)
    })
}

/// One step of the walk to a link label's closing `]`: escapes, code spans,
/// autolinks, and inline HTML are stepped over whole, so brackets inside them
/// do not count; other brackets nest.
fn link_label_step(lookups: &mut impl Lookups, input: &str, cursor: usize) -> BracketStep {
    let Some((next, char)) = next_char(input, cursor) else {
        return BracketStep::End;
    };
    match char {
        '\\' => {
            BracketStep::Pass(next_char(input, next).map_or(next, |(after_escape, _)| after_escape))
        }
        '`' => BracketStep::Pass(code_span_end(lookups, input, cursor).unwrap_or(next)),
        '<' => BracketStep::Pass(
            autolink_end(lookups, input, cursor)
                .or_else(|| html_inline_end(lookups, input, cursor))
                .unwrap_or(next),
        ),
        '[' => BracketStep::Open(next),
        ']' => BracketStep::Close(next),
        _ => BracketStep::Pass(next),
    }
}

fn parse_text_directive(
    input: &str,
    index: usize,
    base_offset: usize,
    options: &SyntaxOptions,
    definitions: Option<Definitions<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
    pass: &mut InlinePass,
) -> Option<(usize, Inline)> {
    if input[index..].starts_with("::") {
        return None;
    }
    if index > 0 {
        let previous = input[..index].chars().next_back()?;
        if !previous.is_whitespace() && !matches!(previous, '(' | '[' | '{') {
            return None;
        }
    }
    let opener_source = &input[index + 1..];
    let opener_offset = index + 1;
    // A name followed by a colon, as in `:word:`, opens no directive.
    let name_len = opener_source
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        .count();
    if opener_source.as_bytes().get(name_len) == Some(&b':') {
        return None;
    }
    let opener = parse_directive_opener_with(opener_source, |close, open| {
        let found = match close {
            DirectiveClose::Label => pass.scan.link_label_end(opener_offset + open),
            DirectiveClose::Attributes => {
                pass.scan.directive_attributes_close(opener_offset + open)
            }
        };
        found.map(|position| position - opener_offset)
    });
    let Some(opener) = opener else {
        if directive_opener_looks_malformed(opener_source) {
            diagnostics.push(Diagnostic::new(
                DiagnosticSeverity::Error,
                DiagnosticCode::InvalidDirectiveName,
                Span::new(base_offset + index, base_offset + input.len()),
                "text directive opener is malformed",
            ));
        }
        return None;
    };
    let label = opener
        .label
        .map(|source| {
            parse_inlines_with_context(
                source,
                base_offset + index + 1 + opener.name.len() + 1,
                options,
                definitions,
                diagnostics,
                pass.state,
            )
        })
        .unwrap_or_default();
    let at = base_offset + opener_offset;
    opener.report_dropped(diagnostics, |start, end| Span::new(at + start, at + end));
    let DirectiveOpener {
        name,
        attributes,
        consumed,
        ..
    } = opener;
    Some((
        index + 1 + consumed,
        Inline::TextDirective(TextDirective {
            meta: NodeMeta::new(Some(Span::new(
                base_offset + index,
                base_offset + index + 1 + consumed,
            ))),
            name,
            label,
            attributes,
        }),
    ))
}

/// A directive's name, label, and attributes, read from the text after its
/// colons.
struct DirectiveOpener<'a> {
    name: String,
    label: Option<&'a str>,
    attributes: Vec<DirectiveAttribute>,
    /// The ranges of the attributes dropped for having no valid name.
    dropped: Vec<(usize, usize)>,
    /// The bytes the opener takes.
    consumed: usize,
}

impl DirectiveOpener<'_> {
    /// A warning for each dropped attribute, spanning `span(start, end)`.
    fn report_dropped(
        &self,
        diagnostics: &mut Vec<Diagnostic>,
        span: impl Fn(usize, usize) -> Span,
    ) {
        for &(start, end) in &self.dropped {
            diagnostics.push(Diagnostic::new(
                DiagnosticSeverity::Warning,
                DiagnosticCode::InvalidDirectiveAttribute,
                span(start, end),
                "directive attribute without a valid name is dropped",
            ));
        }
    }
}

fn parse_directive_opener(input: &str) -> Option<DirectiveOpener<'_>> {
    parse_directive_opener_with(input, |close, open| match close {
        DirectiveClose::Label => find_link_label_end(input, open),
        DirectiveClose::Attributes => find_directive_attributes_close(input, open),
    })
}

/// Which closing position `parse_directive_opener_with` asks its finder for.
#[derive(Clone, Copy)]
enum DirectiveClose {
    /// The `]` closing the `[label]` opening at the given position.
    Label,
    /// The `}` closing the `{attributes}` opening at the given position.
    Attributes,
}

fn parse_directive_opener_with(
    input: &str,
    mut find_close: impl FnMut(DirectiveClose, usize) -> Option<usize>,
) -> Option<DirectiveOpener<'_>> {
    let mut index = 0;
    while let Some((next, char)) = next_char(input, index) {
        if char.is_ascii_alphanumeric() || char == '_' || char == '-' {
            index = next;
        } else {
            break;
        }
    }
    let name = &input[..index];
    if !is_directive_name(name) {
        return None;
    }

    let mut label = None;
    let mut attributes = Vec::new();
    let mut dropped = Vec::new();
    let mut consumed = index;
    if input.as_bytes().get(consumed) == Some(&b'[') {
        let close = find_close(DirectiveClose::Label, consumed)?;
        label = Some(&input[consumed + 1..close]);
        consumed = close + 1;
    }
    if input.as_bytes().get(consumed) == Some(&b'{') {
        let close = find_close(DirectiveClose::Attributes, consumed)?;
        let start = consumed + 1;
        (attributes, dropped) = parse_attributes(&input[start..close]);
        for range in &mut dropped {
            *range = (range.0 + start, range.1 + start);
        }
        consumed = close + 1;
    }

    Some(DirectiveOpener {
        name: name.into(),
        label,
        attributes,
        dropped,
        consumed,
    })
}

fn directive_opener_looks_malformed(input: &str) -> bool {
    let mut index = 0;
    while let Some((next, char)) = next_char(input, index) {
        if char.is_ascii_alphanumeric() || char == '_' || char == '-' {
            index = next;
        } else {
            break;
        }
    }
    index > 0
        && is_directive_name(&input[..index])
        && matches!(input.as_bytes().get(index), Some(b'[' | b'{'))
}

/// Quote states of the walk to a directive's closing `}`: outside quotes,
/// inside `"…"`, inside `'…'`.
const DIRECTIVE_QUOTE_STATES: usize = 3;

fn find_directive_attributes_close(input: &str, open: usize) -> Option<usize> {
    if input.as_bytes().get(open) != Some(&b'{') {
        return None;
    }
    path_walk((open + 1) * DIRECTIVE_QUOTE_STATES, |node| {
        directive_attributes_step(input, node)
    })
}

/// One step of the walk to a directive's closing `}`, over nodes
/// `byte position * DIRECTIVE_QUOTE_STATES + quote state`: a `\` escapes the
/// next byte, and a `}` inside quotes does not close.
fn directive_attributes_step(input: &str, node: usize) -> Step {
    let bytes = input.as_bytes();
    let (cursor, quote) = (node / DIRECTIVE_QUOTE_STATES, node % DIRECTIVE_QUOTE_STATES);
    let at = |position: usize, quote: usize| Step::Next(position * DIRECTIVE_QUOTE_STATES + quote);
    let Some(&byte) = bytes.get(cursor) else {
        return Step::Done(None);
    };
    if byte == b'\\' {
        return at((cursor + 2).min(bytes.len()), quote);
    }
    match (quote, byte) {
        (0, b'"') => at(cursor + 1, 1),
        (0, b'\'') => at(cursor + 1, 2),
        (0, b'}') => Step::Done(Some(cursor)),
        (1, b'"') | (2, b'\'') => at(cursor + 1, 0),
        _ => at(cursor + 1, quote),
    }
}

/// The attributes in `input`, the text between a directive's braces, and the
/// ranges of `input` holding attributes dropped for having no valid name.
fn parse_attributes(input: &str) -> (Vec<DirectiveAttribute>, Vec<(usize, usize)>) {
    let mut attributes = Vec::new();
    let mut dropped = Vec::new();
    let mut cursor = 0;
    while cursor < input.len() {
        cursor = skip_spaces(input, cursor);
        if cursor >= input.len() {
            break;
        }

        if input.as_bytes().get(cursor) == Some(&b'#') {
            let (id, next) = parse_attribute_token(input, cursor + 1);
            if !id.is_empty() {
                attributes.push(DirectiveAttribute {
                    name: "id".into(),
                    value: Some(id.into()),
                });
            }
            cursor = next;
            continue;
        }

        if input.as_bytes().get(cursor) == Some(&b'.') {
            let (class, next) = parse_attribute_token(input, cursor + 1);
            if !class.is_empty() {
                attributes.push(DirectiveAttribute {
                    name: "class".into(),
                    value: Some(class.into()),
                });
            }
            cursor = next;
            continue;
        }

        let token_start = cursor;
        let (name, next) = parse_attribute_name(input, cursor);
        if name.is_empty() {
            dropped.push((token_start, input.trim_end().len().max(token_start)));
            break;
        }
        let mut token_end = next;
        cursor = skip_spaces(input, next);
        let value = if input.as_bytes().get(cursor) == Some(&b'=') {
            cursor = skip_spaces(input, cursor + 1);
            token_end = cursor;
            if let Some((value, next)) = parse_attribute_value(input, cursor) {
                cursor = next;
                token_end = next;
                Some(value)
            } else {
                Some(String::new())
            }
        } else {
            None
        };
        // A token that is no attribute name is dropped, as a malformed value
        // is read leniently: the document keeps only attributes it can write.
        if crate::validate::is_attribute_name(name) {
            attributes.push(DirectiveAttribute {
                name: name.into(),
                value,
            });
        } else {
            dropped.push((token_start, token_end));
        }
    }
    (attributes, dropped)
}

fn parse_attribute_token(input: &str, index: usize) -> (&str, usize) {
    let mut cursor = index;
    while let Some((next, char)) = next_char(input, cursor) {
        if char.is_whitespace() {
            break;
        }
        cursor = next;
    }
    (&input[index..cursor], cursor)
}

fn parse_attribute_name(input: &str, index: usize) -> (&str, usize) {
    let mut cursor = index;
    while let Some((next, char)) = next_char(input, cursor) {
        if char.is_whitespace() || char == '=' {
            break;
        }
        cursor = next;
    }
    (&input[index..cursor], cursor)
}

fn parse_attribute_value(input: &str, index: usize) -> Option<(String, usize)> {
    let quote = input.as_bytes().get(index).copied();
    if matches!(quote, Some(b'"' | b'\'')) {
        let quote = quote?;
        let mut cursor = index + 1;
        while cursor < input.len() {
            let (next, char) = next_char(input, cursor)?;
            if char as u8 == quote && !is_escaped_at(input, cursor) {
                return Some((unescape_ascii_punctuation(&input[index + 1..cursor]), next));
            }
            cursor = next;
        }
        return None;
    }

    let (value, next) = parse_attribute_token(input, index);
    Some((
        unescape_selected(value, |char| matches!(char, '\\' | '&')),
        next,
    ))
}

struct CodeSpanSource {
    value: String,
    raw: String,
    fence_length: usize,
}

fn parse_code_span_with(
    lookups: &mut impl Lookups,
    input: &str,
    index: usize,
) -> Option<(usize, CodeSpanSource)> {
    let len = backtick_run_len(input, index);
    let search_start = index + len;
    let close = lookups.code_span_close(search_start, len)?;
    let raw = &input[search_start..close];
    Some((
        close + len,
        CodeSpanSource {
            value: normalize_code_span(raw),
            raw: raw.into(),
            fence_length: len,
        },
    ))
}

/// The end of the code span opening at `index`, as `parse_code_span` finds it.
fn code_span_end(lookups: &mut impl Lookups, input: &str, index: usize) -> Option<usize> {
    let len = backtick_run_len(input, index);
    Some(lookups.code_span_close(index + len, len)? + len)
}

fn find_code_span_close(input: &str, start: usize, marker_len: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut cursor = start;
    while cursor < bytes.len() {
        if bytes[cursor] != b'`' {
            cursor = next_char(input, cursor)
                .map(|(next, _)| next)
                .unwrap_or(bytes.len());
            continue;
        }
        let run_len = bytes[cursor..]
            .iter()
            .take_while(|byte| **byte == b'`')
            .count();
        if run_len == marker_len {
            return Some(cursor);
        }
        cursor += run_len;
    }
    None
}

fn normalize_code_span(input: &str) -> String {
    let mut normalized = String::new();
    let mut cursor = 0;
    while cursor < input.len() {
        let (next, char) = next_char(input, cursor).expect("valid UTF-8 byte index");
        if char == '\r' {
            if input.as_bytes().get(next) == Some(&b'\n') {
                cursor = next + 1;
            } else {
                cursor = next;
            }
            normalized.push(' ');
            continue;
        }
        if char == '\n' {
            normalized.push(' ');
            cursor = next;
            continue;
        }
        normalized.push(char);
        cursor = next;
    }

    if normalized.starts_with(' ')
        && normalized.ends_with(' ')
        && normalized.chars().any(|char| char != ' ')
    {
        normalized[1..normalized.len() - 1].into()
    } else {
        normalized
    }
}

fn can_open_delimited(input: &str, index: usize, marker_len: usize) -> bool {
    delimiter_flanking(input, index, marker_len).left
}

fn can_close_delimited(input: &str, index: usize, marker_len: usize) -> bool {
    delimiter_flanking(input, index, marker_len).right
}

fn is_single_tilde_delete_close(input: &str, index: usize) -> bool {
    input.as_bytes()[index] == b'~'
        && !is_escaped_at(input, index)
        && single_tilde_can_close_delete(input, index)
}

fn single_tilde_can_open_delete(input: &str, index: usize) -> bool {
    starts_exact_byte_run(input, index, b'~', 1)
        && can_open_delimited(input, index, 1)
        && !tilde_is_alphanumeric_interior(input, index)
}

fn single_tilde_can_close_delete(input: &str, index: usize) -> bool {
    starts_exact_byte_run(input, index, b'~', 1)
        && can_close_delimited(input, index, 1)
        && !tilde_is_alphanumeric_interior(input, index)
}

fn single_tilde_delete_takes_precedence(
    options: &SyntaxOptions,
    input: &str,
    index: usize,
    scan: &mut InlineScan,
) -> bool {
    options.constructs.gfm_strikethrough
        && options.parse.single_tilde_strikethrough
        && single_tilde_can_open_delete(input, index)
        && scan.single_tilde_delete_close(index + 1).is_some()
}

fn tilde_is_alphanumeric_interior(input: &str, index: usize) -> bool {
    let previous = input[..index].chars().next_back();
    let next = input[index + 1..].chars().next();
    previous.is_some_and(|char| char.is_alphanumeric())
        && next.is_some_and(|char| char.is_alphanumeric())
}

fn starts_exact_byte_run(input: &str, index: usize, marker: u8, len: usize) -> bool {
    input.as_bytes().get(index) == Some(&marker)
        && delimiter_byte_run_start(input, index, marker) == index
        && delimiter_byte_run_len(input, index, marker) == len
}

fn delimiter_byte_run_start(input: &str, index: usize, marker: u8) -> usize {
    let bytes = input.as_bytes();
    let mut start = index;
    while start > 0 && bytes[start - 1] == marker && !is_escaped_at(input, start - 1) {
        start -= 1;
    }
    start
}

fn delimiter_byte_run_len(input: &str, index: usize, marker: u8) -> usize {
    let bytes = input.as_bytes();
    let mut cursor = index;
    while bytes.get(cursor) == Some(&marker) {
        cursor += 1;
    }
    cursor - index
}

/// Whether a run of `_` that the `marker_len` bytes of `input` from `index`
/// are can open and can close emphasis, as the parser reads it.
pub(crate) fn underscore_run_flanks(input: &str, index: usize, marker_len: usize) -> (bool, bool) {
    (
        can_open_underscore(input, index, marker_len),
        can_close_underscore(input, index, marker_len),
    )
}

fn can_open_underscore(input: &str, index: usize, marker_len: usize) -> bool {
    let flanking = delimiter_flanking(input, index, marker_len);
    flanking.left && (!flanking.right || flanking.previous.is_some_and(is_flanking_punctuation))
}

fn can_close_underscore(input: &str, index: usize, marker_len: usize) -> bool {
    let flanking = delimiter_flanking(input, index, marker_len);
    flanking.right && (!flanking.left || flanking.next.is_some_and(is_flanking_punctuation))
}

#[derive(Clone, Copy)]
struct DelimiterFlanking {
    left: bool,
    right: bool,
    previous: Option<char>,
    next: Option<char>,
}

fn delimiter_flanking(input: &str, index: usize, marker_len: usize) -> DelimiterFlanking {
    delimiter_flanking_within(input, index, marker_len, false, false)
}

/// `delimiter_flanking` for a run that may sit right inside a mark span: a
/// bounded side flanks as if the input ended there, the way the span's content
/// reads on its own.
fn delimiter_flanking_within(
    input: &str,
    index: usize,
    marker_len: usize,
    bounded_before: bool,
    bounded_after: bool,
) -> DelimiterFlanking {
    let previous = if bounded_before {
        None
    } else {
        input[..index].chars().next_back().map(source_char)
    };
    let next = if bounded_after {
        None
    } else {
        input[index + marker_len..].chars().next().map(source_char)
    };

    let previous_whitespace = previous.is_none_or(char::is_whitespace);
    let next_whitespace = next.is_none_or(char::is_whitespace);
    let previous_punctuation = previous.is_some_and(is_flanking_punctuation);
    let next_punctuation = next.is_some_and(is_flanking_punctuation);

    let left = next.is_some()
        && !next_whitespace
        && !(next_punctuation && !previous_whitespace && !previous_punctuation);
    let right = previous.is_some()
        && !previous_whitespace
        && !(previous_punctuation && !next_whitespace && !next_punctuation);

    DelimiterFlanking {
        left,
        right,
        previous,
        next,
    }
}

/// Dollar-fenced inline math, GitHub Flavored Markdown dialect.
///
/// A `$` is a flanking delimiter resolved at scan time (math is not pushed onto
/// the emphasis delimiter stack). An opening run of one or two `$` (runs of
/// three or more never form math) scans forward for a matching closing run:
///
/// * single `$`: cannot open if the next char is ASCII whitespace; the closing
///   `$` cannot be preceded by ASCII whitespace nor followed by an ASCII digit;
///   a `\$` inside is skipped (the backslash is kept verbatim, never a
///   delimiter); the close must be a run of exactly one `$`.
/// * double `$$`: no flanking and no digit guard; closes on the next run of two
///   `$`; content is kept verbatim and may span newlines (this is still an
///   inline display span — `$$` flow blocks are handled by `parse_math_block`).
///
/// The closing run is matched greedily (the nearest valid close wins), which is
/// equivalent to emphasis-style "nearest preceding open" because a failed open
/// emits a literal `$`/`$$` and the scan resumes after it. Content for the
/// single-`$` form is normalized like a code span (line endings → spaces, one
/// edge-space strip); the `$$` display form is verbatim. The `` $`…`$ `` code
/// form takes precedence.
fn parse_math_inline(
    lookups: &mut impl Lookups,
    input: &str,
    index: usize,
) -> Option<(usize, String, MathInlineKind)> {
    if let Some((end, value)) = parse_math_code_inline(lookups, input, index) {
        return Some((end, value, MathInlineKind::Code));
    }

    let bytes = input.as_bytes();
    let open_dollars = bytes[index..]
        .iter()
        .take_while(|byte| **byte == b'$')
        .count();
    // The maximum math fence length is 2 dollars: a run of three or more never
    // opens math.
    if open_dollars == 0 || open_dollars > 2 {
        return None;
    }

    let content_start = index + open_dollars;
    let close = scan_to_closing_dollar(input, content_start, open_dollars)?;
    let content_end = close - open_dollars;
    // The span requires `endpos - startpos >= fence_length * 2 + 1`, i.e. at
    // least one content byte between the open and close fences.
    if content_end <= content_start {
        return None;
    }

    let raw = &input[content_start..content_end];
    let value = if open_dollars == 1 {
        normalize_math_text(raw)
    } else {
        raw.into()
    };
    let dollars = u8::try_from(open_dollars).unwrap_or(u8::MAX);
    Some((close, value, MathInlineKind::Dollar { dollars }))
}

/// Scans for the closing dollar run. `start` is the first content byte
/// (just past the opening run); returns the byte offset just past a matching
/// closing run of exactly `open_dollars` `$`.
fn scan_to_closing_dollar(input: &str, start: usize, open_dollars: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    // A space immediately after a single opening `$` forbids the open.
    if open_dollars == 1 && bytes.get(start).is_some_and(|byte| is_math_space(*byte)) {
        return None;
    }

    let mut cursor = start;
    loop {
        while cursor < bytes.len() && bytes[cursor] != b'$' {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            return None;
        }
        // `cursor` now points at the first `$` of a potential closing run; the
        // char just before it gates the single-`$` flanking and escape rules.
        let prev = bytes[cursor - 1];
        if open_dollars == 1 && is_math_space(prev) {
            return None;
        }
        if open_dollars == 1 && prev == b'\\' {
            // An escaped `\$` is content, not a delimiter: skip this one `$` and
            // keep scanning (the backslash stays in the content verbatim).
            cursor += 1;
            continue;
        }
        let run = bytes[cursor..]
            .iter()
            .take(open_dollars)
            .take_while(|byte| **byte == b'$')
            .count();
        // The single-`$` close cannot be followed by an ASCII digit.
        if open_dollars == 1 && bytes.get(cursor + run).is_some_and(u8::is_ascii_digit) {
            return None;
        }
        if run == open_dollars {
            return Some(cursor + run);
        }
        cursor += run;
    }
}

/// Math whitespace: ASCII tab, line feed, carriage return, and space.
fn is_math_space(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\r' | b' ')
}

/// Applies the code-span content rules to dollar-fenced math: line endings
/// become single spaces, then if the content begins AND ends with U+0020 and is
/// not entirely spaces, one space is stripped from each edge.
fn normalize_math_text(input: &str) -> String {
    let mut normalized = String::new();
    let mut cursor = 0;
    while cursor < input.len() {
        let (next, char) = next_char(input, cursor).expect("valid UTF-8 byte index");
        if char == '\r' {
            if input.as_bytes().get(next) == Some(&b'\n') {
                cursor = next + 1;
            } else {
                cursor = next;
            }
            normalized.push(' ');
            continue;
        }
        if char == '\n' {
            normalized.push(' ');
            cursor = next;
            continue;
        }
        normalized.push(char);
        cursor = next;
    }

    if normalized.starts_with(' ')
        && normalized.ends_with(' ')
        && normalized.chars().any(|char| char != ' ')
    {
        normalized[1..normalized.len() - 1].into()
    } else {
        normalized
    }
}

fn parse_math_code_inline(
    lookups: &mut impl Lookups,
    input: &str,
    index: usize,
) -> Option<(usize, String)> {
    if !input[index..].starts_with("$`") {
        return None;
    }

    let search_start = index + 2;
    let close = lookups.find("`$", search_start)?;
    if close == search_start {
        return None;
    }

    Some((close + 2, input[search_start..close].into()))
}

fn parse_link_resource(
    lookups: &mut impl Lookups,
    input: &str,
    open: usize,
) -> Option<(usize, ParsedLinkResource)> {
    let bytes = input.as_bytes();
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let (mut cursor, initial_space) = skip_link_resource_space_with_info(input, open + 1)?;
    if bytes.get(cursor) == Some(&b')') {
        return Some((
            cursor + 1,
            ParsedLinkResource {
                destination: String::new(),
                destination_kind: LinkDestinationKind::Omitted,
                title: None,
                title_kind: None,
            },
        ));
    }
    if initial_space && matches!(bytes.get(cursor), Some(b'"' | b'\'' | b'(')) {
        let (title, title_kind, next) = parse_link_title(input, cursor)?;
        cursor = skip_link_resource_space(input, next)?;
        if bytes.get(cursor) == Some(&b')') {
            return Some((
                cursor + 1,
                ParsedLinkResource {
                    destination: String::new(),
                    destination_kind: LinkDestinationKind::Omitted,
                    title: Some(title),
                    title_kind: Some(title_kind),
                },
            ));
        }
        return None;
    }
    let (destination, destination_kind, next) = parse_link_destination(lookups, input, cursor)?;
    let (after_destination, had_space) = skip_link_resource_space_with_info(input, next)?;
    cursor = after_destination;
    if bytes.get(cursor) == Some(&b')') {
        return Some((
            cursor + 1,
            ParsedLinkResource {
                destination,
                destination_kind,
                title: None,
                title_kind: None,
            },
        ));
    }
    if !had_space {
        return None;
    }

    let (title, title_kind, next) = parse_link_title(input, cursor)?;
    cursor = skip_link_resource_space(input, next)?;
    if bytes.get(cursor) == Some(&b')') {
        Some((
            cursor + 1,
            ParsedLinkResource {
                destination,
                destination_kind,
                title: Some(title),
                title_kind: Some(title_kind),
            },
        ))
    } else {
        None
    }
}

fn parse_link_destination(
    lookups: &mut impl Lookups,
    input: &str,
    index: usize,
) -> Option<(String, LinkDestinationKind, usize)> {
    if input.as_bytes().get(index) == Some(&b'<') {
        let stop = lookups.angle_destination_stop(index + 1)?;
        if input.as_bytes()[stop] != b'>' {
            return None;
        }
        return Some((
            unescape_ascii_punctuation(&input[index + 1..stop]),
            LinkDestinationKind::Angle,
            stop + 1,
        ));
    }

    let mut cursor = index;
    let mut depth = 0usize;
    while cursor < input.len() {
        let (next, char) = next_char(input, cursor)?;
        // A bare destination terminates on ASCII space or an ASCII control
        // character; Unicode whitespace (e.g. U+00A0) is ordinary. A backslash
        // before a space is NOT an escape (only ASCII punctuation is escapable),
        // so `\ ` still terminates the destination → `[a](\ b)` is not a link.
        if char == ' ' || source_char(char).is_ascii_control() {
            break;
        }
        if char == '(' && !is_escaped_at(input, cursor) {
            depth += 1;
            // CommonMark caps balanced parens in a bare destination at depth 32.
            if depth > 32 {
                return None;
            }
        } else if char == ')' && !is_escaped_at(input, cursor) {
            if depth == 0 {
                break;
            }
            depth -= 1;
        }
        cursor = next;
    }

    if cursor == index || depth > 0 {
        None
    } else {
        Some((
            unescape_ascii_punctuation(&input[index..cursor]),
            LinkDestinationKind::Bare,
            cursor,
        ))
    }
}

fn parse_link_title(input: &str, index: usize) -> Option<(String, LinkTitleKind, usize)> {
    let opener = input.as_bytes().get(index).copied()?;
    let (closer, title_kind) = match opener {
        b'"' => ('"', LinkTitleKind::DoubleQuote),
        b'\'' => ('\'', LinkTitleKind::SingleQuote),
        b'(' => (')', LinkTitleKind::Paren),
        _ => return None,
    };
    let mut cursor = index + 1;
    while cursor < input.len() {
        let (next, char) = next_char(input, cursor)?;
        if char == closer && !is_escaped_at(input, cursor) {
            if contains_blank_line(&input[index + 1..cursor]) {
                return None;
            }
            return Some((
                unescape_ascii_punctuation(&input[index + 1..cursor]),
                title_kind,
                next,
            ));
        }
        if opener == b'(' && char == '(' && !is_escaped_at(input, cursor) {
            return None;
        }
        cursor = next;
    }
    None
}

fn contains_blank_line(input: &str) -> bool {
    if !input.bytes().any(|byte| matches!(byte, b'\n' | b'\r')) {
        return false;
    }
    // A title that merely begins or ends with an EOL is allowed; only an INTERIOR
    // blank line (a blank line bounded by content on both sides) is rejected. The
    // empty first/last line entries that a leading/trailing newline produces are
    // boundary artifacts, not blank lines in the title content.
    let map = SourceMap::verbatim(input.len(), 0);
    let lines = collect_lines(input, &map);
    let interior = lines.len().saturating_sub(1);
    lines
        .iter()
        .take(interior)
        .skip(1)
        .any(|line| is_blank(line.text))
}

fn skip_link_resource_space(input: &str, index: usize) -> Option<usize> {
    skip_link_resource_space_with_info(input, index).map(|(index, _)| index)
}

fn skip_link_resource_space_with_info(input: &str, mut index: usize) -> Option<(usize, bool)> {
    let mut line_breaks = 0usize;
    let mut had_space = false;
    while input
        .as_bytes()
        .get(index)
        .is_some_and(|byte| matches!(*byte, b' ' | b'\t' | b'\n' | b'\r'))
    {
        had_space = true;
        match input.as_bytes()[index] {
            b'\n' => {
                line_breaks += 1;
                if line_breaks > 1 {
                    return None;
                }
                index += 1;
            }
            b'\r' => {
                line_breaks += 1;
                if line_breaks > 1 {
                    return None;
                }
                if input.as_bytes().get(index + 1) == Some(&b'\n') {
                    index += 2;
                } else {
                    index += 1;
                }
            }
            _ => index += 1,
        }
    }
    Some((index, had_space))
}

pub(crate) fn parse_character_reference(input: &str, index: usize) -> Option<(usize, String)> {
    let rest = input.get(index..)?;
    if let Some(rest) = rest
        .strip_prefix("&#x")
        .or_else(|| rest.strip_prefix("&#X"))
    {
        let digits = find_reference_terminator(rest, 6)?;
        if digits == 0 || !rest[..digits].bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let value = u32::from_str_radix(&rest[..digits], 16).ok()?;
        return Some((
            index + 3 + digits + 1,
            character_reference_value(value).into(),
        ));
    }
    if let Some(rest) = rest.strip_prefix("&#") {
        let digits = find_reference_terminator(rest, 7)?;
        if digits == 0 || !rest[..digits].bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let value = rest[..digits].parse::<u32>().ok()?;
        return Some((
            index + 2 + digits + 1,
            character_reference_value(value).into(),
        ));
    }

    let name_end = find_reference_terminator(rest, 32)?;
    if name_end == 0 {
        return None;
    }
    let name = &rest[1..name_end];
    named_character_reference(name).map(|value| (index + name_end + 1, value.into()))
}

/// Finds the `;` that ends a character reference body, looking no further than
/// `max_len` bytes: a longer body is never a reference, so the scan stays
/// bounded instead of running to the end of the input.
fn find_reference_terminator(rest: &str, max_len: usize) -> Option<usize> {
    rest.as_bytes()
        .iter()
        .take(max_len + 1)
        .position(|byte| *byte == b';')
}

/// Decode a numeric character reference codepoint to its scalar value.
///
/// This follows the CommonMark reference behavior: `U+0000`, the UTF-16
/// surrogate range, and codepoints beyond the Unicode scalar range decode to
/// `U+FFFD`; every other codepoint decodes to itself.
///
/// Two deliberate non-behaviors:
/// - We do NOT apply the HTML5 Windows-1252 remapping of C1 bytes; `&#128;`
///   decodes to `U+0080`, not the Euro sign. The CommonMark reference does not
///   perform that remapping.
/// - We do NOT extend replacement to the C0/C1 controls, DEL, or the Unicode
///   noncharacters the way some HTML-oriented decoders do. Keeping those as
///   their literal scalar is what makes the serializer's `&#xNN;` escaping of
///   control characters round-trip through a re-parse. The roundtrip corpus
///   only pins `{0 -> FFFD, 9 -> tab, 10 -> line feed, surrogate -> FFFD,
///   out-of-range -> FFFD}`, all of which this matches.
pub(crate) fn character_reference_value(value: u32) -> char {
    if value == 0 {
        '\u{FFFD}'
    } else {
        char::from_u32(value).unwrap_or('\u{FFFD}')
    }
}

pub(crate) fn is_escaped_at(input: &str, index: usize) -> bool {
    let bytes = input.as_bytes();
    let mut cursor = index;
    let mut count = 0;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        count += 1;
        cursor -= 1;
    }
    count % 2 == 1
}

fn parse_definition_destination_title(input: &str) -> Option<ParsedLinkResource> {
    let (mut cursor, _) = skip_link_resource_space_with_info(input, 0)?;
    let (destination, destination_kind, next) =
        parse_link_destination(&mut DirectLookups { input }, input, cursor)?;
    cursor = next;

    let (next, had_space) = skip_link_resource_space_with_info(input, cursor)?;
    cursor = next;
    if cursor >= input.len() {
        return Some(ParsedLinkResource {
            destination,
            destination_kind,
            title: None,
            title_kind: None,
        });
    }
    if !had_space {
        return None;
    }

    let (title, title_kind, next) = parse_link_title(input, cursor)?;
    let after_title = skip_link_resource_space(input, next)?;
    (after_title == input.len()).then_some(ParsedLinkResource {
        destination,
        destination_kind,
        title: Some(title),
        title_kind: Some(title_kind),
    })
}

/// The char that closes the title `source` opens after its destination, when
/// no unescaped one follows the opener yet.
fn open_definition_title(source: &str) -> Option<char> {
    let (cursor, _) = skip_link_resource_space_with_info(source, 0)?;
    let (_, _, next) =
        parse_link_destination(&mut DirectLookups { input: source }, source, cursor)?;
    let (cursor, had_space) = skip_link_resource_space_with_info(source, next)?;
    if !had_space {
        return None;
    }
    let closer = match source.as_bytes().get(cursor)? {
        b'"' => '"',
        b'\'' => '\'',
        b'(' => ')',
        _ => return None,
    };
    (!line_may_close_title(&source[cursor + 1..], closer)).then_some(closer)
}

/// Whether `line` holds an unescaped `closer`, or an unescaped `(` inside a
/// parenthesized title, either of which settles the title. A backslash
/// escape never spans a line ending, so each line is read alone.
fn line_may_close_title(line: &str, closer: char) -> bool {
    line.char_indices().any(|(index, char)| {
        (char == closer || (closer == ')' && char == '(')) && !is_escaped_at(line, index)
    })
}

fn line_can_start_definition_title(input: &str) -> bool {
    let trimmed = trim_ascii_start(input);
    matches!(trimmed.as_bytes().first(), Some(b'"' | b'\'' | b'('))
}

fn unescape_ascii_punctuation(input: &str) -> String {
    // Only ASCII punctuation is escapable (`\ ` keeps its backslash).
    unescape_selected(input, |char| char.is_ascii_punctuation())
}

fn unescape_string(input: &str) -> String {
    unescape_selected(input, |char| char.is_ascii_punctuation() || char == '&')
}

fn unescape_selected(input: &str, should_unescape: impl Fn(char) -> bool) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    while cursor < input.len() {
        if input.as_bytes().get(cursor) == Some(&b'&') {
            if let Some((end, value)) = parse_character_reference(input, cursor) {
                output.push_str(&value);
                cursor = end;
                continue;
            }
        }
        let (next, char) = next_char(input, cursor).expect("valid UTF-8 byte index");
        if char == '\\' {
            if let Some((after_escape, escaped)) = next_char(input, next) {
                if should_unescape(escaped) {
                    output.push(escaped);
                } else {
                    output.push(char);
                    output.push(escaped);
                }
                cursor = after_escape;
            } else {
                output.push(char);
                cursor = next;
            }
        } else {
            output.push(char);
            cursor = next;
        }
    }
    output
}

fn push_line(output: &mut String, line: &str) {
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str(line);
}

/// Ends a code or math block's last line when the input ended without a line
/// ending: every line of such a block's value ends with one. The added ending
/// repeats the value's first line ending, `\n` when it has none.
fn end_last_line(value: &mut String) {
    if value.is_empty() || ends_with_line_ending(value) {
        return;
    }
    value.push_str(value_line_ending(value));
}

/// The line ending a code or math value's lines take: its first one, `\n`
/// when it has none.
fn value_line_ending(value: &str) -> &'static str {
    match value.find(['\r', '\n']) {
        Some(index) if value[index..].starts_with("\r\n") => "\r\n",
        Some(index) if value[index..].starts_with('\r') => "\r",
        _ => "\n",
    }
}

fn ends_with_line_ending(input: &str) -> bool {
    input.ends_with('\n') || input.ends_with('\r')
}

fn flush_text(nodes: &mut Vec<Inline>, text: &mut String, text_start: usize, end: usize) {
    if !text.is_empty() {
        nodes.push(Inline::Text(Text {
            meta: NodeMeta::new(Some(Span::new(text_start, end))),
            value: core::mem::take(text),
        }));
    }
}

fn next_char(input: &str, index: usize) -> Option<(usize, char)> {
    let char = input[index..].chars().next()?;
    Some((index + char.len_utf8(), char))
}

/// A CommonMark "Unicode punctuation character" for emphasis/strong flanking:
/// ASCII punctuation plus the non-ASCII Unicode `P*`/`S*` categories. Only the
/// flanking classification needs the Unicode set; escape/label logic stays
/// ASCII-only via `char::is_ascii_punctuation`.
fn is_flanking_punctuation(value: char) -> bool {
    value.is_ascii_punctuation() || crate::unicode_punctuation::is_unicode_punctuation(value)
}

/// Fold a reference label to its matching identifier. Per CommonMark, two
/// labels match when their RAW source (no backslash unescape, no entity decode)
/// agrees after collapsing internal spaces, tabs, and line endings to a single
/// space, trimming, and Unicode case-folding (`to_uppercase()` then
/// `to_lowercase()`). So `[foo\!]`
/// does NOT match `[foo!]`, and `[&copy;]` does NOT match `[©]`.
///
/// The serializer's `normalize_reference_label` delegates here so the
/// Shortcut/Collapsed omission oracle stays in lockstep with this matcher.
pub(crate) fn normalize_label(label: &str) -> String {
    label
        // Unicode full casefold maps capital sharp S (ẞ, U+1E9E) to "ss"; Rust's
        // `to_uppercase` leaves it unchanged (it is already uppercase), so without
        // this `[ẞ]` would not match a `[SS]: …` definition (links 540). This is
        // the only char where `to_uppercase().to_lowercase()` diverges from the
        // full casefold that matters for label matching.
        .replace('ẞ', "ss")
        .replace('\0', "\u{FFFD}")
        .split([' ', '\t', '\n', '\r'])
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
        .to_lowercase()
}

/// The identifiers a document's references can resolve to: those of its own
/// definitions, and those a caller knows of, each sorted and deduplicated so
/// that a lookup is a binary search.
#[derive(Clone, Copy)]
pub(crate) struct Definitions<'a> {
    pub(crate) own: &'a [String],
    pub(crate) known: &'a [String],
}

fn definition_exists(definitions: Option<Definitions<'_>>, label: &str) -> bool {
    if label.is_empty() || !reference_label_is_within_limit(label) {
        return false;
    }
    let Some(definitions) = definitions else {
        return false;
    };
    let identifier = normalize_label(label);
    definitions.own.binary_search(&identifier).is_ok()
        || definitions.known.binary_search(&identifier).is_ok()
}

fn reference_label_is_within_limit(label: &str) -> bool {
    label.chars().take(REFERENCE_LABEL_MAX_CHARS + 1).count() <= REFERENCE_LABEL_MAX_CHARS
}

fn trim_up_to_three_spaces(input: &str) -> Option<&str> {
    let (columns, bytes) = leading_indent(input);
    if columns <= 3 {
        Some(&input[bytes..])
    } else {
        None
    }
}

/// The marker and length of the code fence that `input` opens: three or more
/// backticks or tildes, where a backtick fence's info string holds no backtick.
fn fence_start(input: &str) -> Option<(FenceMarker, usize)> {
    let marker = match input.as_bytes().first()? {
        b'`' => FenceMarker::Backtick,
        b'~' => FenceMarker::Tilde,
        _ => return None,
    };
    let byte = match marker {
        FenceMarker::Backtick => b'`',
        FenceMarker::Tilde => b'~',
    };
    let length = input
        .as_bytes()
        .iter()
        .take_while(|item| **item == byte)
        .count();
    let opens = length >= 3 && (byte == b'~' || !input[length..].contains('`'));
    opens.then_some((marker, length))
}

fn fence_close(input: &str, marker: FenceMarker, length: usize) -> bool {
    let byte = match marker {
        FenceMarker::Backtick => b'`',
        FenceMarker::Tilde => b'~',
    };
    let count = input
        .as_bytes()
        .iter()
        .take_while(|item| **item == byte)
        .count();
    count >= length && is_blank(&input[count..])
}

fn trim_closing_hashes(input: &str) -> &str {
    let input = input.trim_end_matches([' ', '\t']);
    let hash_start = input.trim_end_matches('#').len();
    if hash_start == input.len() {
        return input;
    }
    if hash_start == 0 {
        return "";
    }

    let before = &input[..hash_start];
    if before.ends_with(' ') || before.ends_with('\t') {
        before.trim_end_matches([' ', '\t'])
    } else {
        input
    }
}

/// Whether `input` opens with a list item marker, up to three spaces in.
fn opens_list_item(input: &str) -> bool {
    trim_up_to_three_spaces(input).is_some_and(|rest| blocks::list_marker_head(rest).is_some())
}

/// The offset of `slice` inside `text` when it is a borrowed sub-slice of it.
fn slice_offset_in(text: &str, slice: &str) -> Option<usize> {
    let offset = (slice.as_ptr() as usize).checked_sub(text.as_ptr() as usize)?;
    (offset + slice.len() <= text.len()).then_some(offset)
}

fn leading_indent(input: &str) -> (usize, usize) {
    leading_indent_at(input, 0)
}

/// The columns and bytes of `input`'s leading spaces and tabs, its first
/// char at source column `start`, from which tabs reach their stops.
fn leading_indent_at(input: &str, start: usize) -> (usize, usize) {
    let mut column = start;
    let mut bytes = 0usize;
    for byte in input.as_bytes() {
        match *byte {
            b' ' => column += 1,
            b'\t' => column += 4 - (column % 4),
            _ => break,
        }
        bytes += 1;
    }
    (column - start, bytes)
}

fn task_marker_checked(input: &str) -> Option<bool> {
    if input.starts_with("[ ]") {
        Some(false)
    } else if input.starts_with("[x]") || input.starts_with("[X]") {
        Some(true)
    } else {
        None
    }
}

/// Whether `text` holds only spaces and tabs, as a blank line does.
fn is_blank(text: &str) -> bool {
    text.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
}

fn trim_ascii_start(input: &str) -> &str {
    input.trim_start_matches(|char| matches!(char, ' ' | '\t'))
}

fn leading_trim_bytes(input: &str) -> usize {
    input.len() - trim_ascii_start(input).len()
}

fn parse_table_delimiter(input: &str, spoiler: bool) -> Option<Vec<TableAlignment>> {
    // Every cell trims to colons around dashes, so a row with any other char
    // is no delimiter row, whatever its cells.
    if !input.contains('-')
        || !input
            .chars()
            .all(|char| matches!(char, '|' | '-' | ':' | ' ' | '\t'))
    {
        return None;
    }
    let cells = table_row_cell_ranges(input, spoiler);
    if cells.is_empty() {
        return None;
    }
    let mut alignments = Vec::new();
    for (start, end) in cells {
        alignments.push(table_delimiter_alignment(
            input[start..end].trim_matches([' ', '\t']),
        )?);
    }
    Some(alignments)
}

// A delimiter cell is `:?` `-`+ `:?` once trimmed: colons only at the
// boundaries, the dashes contiguous, no interior space or colon.
fn table_delimiter_alignment(cell: &str) -> Option<TableAlignment> {
    let bytes = cell.as_bytes();
    let mut cursor = 0;
    let left = bytes.first() == Some(&b':');
    if left {
        cursor += 1;
    }
    let dash_start = cursor;
    while bytes.get(cursor) == Some(&b'-') {
        cursor += 1;
    }
    if cursor == dash_start {
        return None;
    }
    let right = bytes.get(cursor) == Some(&b':');
    if right {
        cursor += 1;
    }
    if cursor != bytes.len() {
        return None;
    }
    Some(match (left, right) {
        (true, true) => TableAlignment::Center,
        (true, false) => TableAlignment::Left,
        (false, true) => TableAlignment::Right,
        (false, false) => TableAlignment::None,
    })
}

/// The cell-delimiter pipes of a table row, in order: every `|` that is not
/// escaped by an odd backslash run and not held inside a cell by a code span or
/// a spoiler.
///
/// A single unescaped pipe always delimits, even inside a code span; a code
/// span only keeps the bars of a `||` from delimiting. With spoilers enabled,
/// bar runs outside code spans pair first-closer style, as the inline parser
/// pairs them within one cell: a run opens with its last two bars, the next run
/// closes with its first two (and opens again with what it has left), and every
/// pipe between the two stays in the cell. The other bars of a run, and both
/// bars of an opener with no closer, delimit. An escaped pipe is an escape in
/// the cell's inline content: it never delimits and is no bar of a run. Code
/// spans are matched as the inline parser matches them, and only when the span
/// closes before any pipe that would delimit inside it, so it lies within one
/// cell. The pairing does not consider other inline constructs.
fn table_row_delimiters(row: &str, spoiler: bool) -> Vec<usize> {
    scan_table_row(row, spoiler).0
}

/// The delimiters of a table row and the spans of the spoilers its cells will
/// hold, from each opener's first bar to its closer's last.
fn scan_table_row(row: &str, spoiler: bool) -> (Vec<usize>, Vec<(usize, usize)>) {
    let bytes = row.as_bytes();

    // Every pipe, grouped into the bar runs of the cell text.
    let mut bars: Vec<TableBar> = Vec::new();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut backticks = false;
    let mut cursor = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => {
                let backslashes = delimiter_byte_run_len(row, cursor, b'\\');
                let pipe = cursor + backslashes;
                if bytes.get(pipe) == Some(&b'|') && backslashes % 2 == 1 {
                    // The cell text drops one backslash, so with exactly one
                    // the pipe follows whatever precedes the run.
                    let text_start = if backslashes == 1 { cursor } else { pipe };
                    push_table_bar(&mut bars, &mut runs, pipe, text_start, true);
                    cursor = pipe + 1;
                } else {
                    cursor = pipe;
                }
            }
            b'|' => {
                push_table_bar(&mut bars, &mut runs, cursor, cursor, false);
                cursor += 1;
            }
            byte => {
                backticks |= byte == b'`';
                cursor += 1;
            }
        }
    }
    if !spoiler || (!backticks && runs.iter().all(|&(_, length)| length == 1)) {
        // With no bar runs and no code spans, every unescaped pipe delimits.
        let delimiters = bars
            .iter()
            .filter(|bar| !bar.escaped)
            .map(|bar| bar.position)
            .collect();
        return (delimiters, Vec::new());
    }

    // Inside a code span, two unescaped bars side by side stay in the cell and
    // any other unescaped bar delimits.
    let mut code_delimits = vec![false; bars.len()];
    for &(first, length) in &runs {
        let mut bar = first;
        while bar < first + length {
            if bar + 1 < first + length && !bars[bar].escaped && !bars[bar + 1].escaped {
                bar += 2;
            } else {
                code_delimits[bar] = !bars[bar].escaped;
                bar += 1;
            }
        }
    }

    // A code span forms only when no bar that would delimit inside it lies
    // before its closer.
    let mut breaker = 0;
    let code_spans = if backticks {
        inline_code_spans(row, |open, close| {
            while breaker < bars.len() && (bars[breaker].position < open || !code_delimits[breaker])
            {
                breaker += 1;
            }
            bars.get(breaker).is_none_or(|bar| close < bar.position)
        })
    } else {
        Vec::new()
    };

    // Decide which bars delimit; `held` collects the ranges a spoiler keeps in
    // its cell.
    let mut delimits = vec![false; bars.len()];
    let mut held = Vec::new();
    let mut opener: Option<usize> = None;
    let mut code = code_spans.iter().peekable();
    for (first, length) in runs {
        let end = first + length;
        let position = bars[first].position;
        while code.peek().is_some_and(|&&(_, close)| close < position) {
            code.next();
        }
        if code.peek().is_some_and(|&&(open, _)| open < position) {
            delimits[first..end].copy_from_slice(&code_delimits[first..end]);
            continue;
        }
        if length == 1 {
            delimits[first] = true;
            continue;
        }
        let mut rest = first;
        if let Some(opened) = opener.take() {
            held.push((bars[opened].position, bars[first + 1].position));
            rest = first + 2;
        }
        let mut open_at = end;
        if end - rest >= 2 {
            open_at = end - 2;
            opener = Some(open_at);
        }
        delimits[rest..open_at].fill(true);
    }
    if let Some(opened) = opener {
        delimits[opened..opened + 2].fill(true);
    }

    let mut spoilers = held.iter().copied().peekable();
    let mut delimiters = Vec::new();
    for (bar, delimit) in bars.iter().zip(delimits) {
        while spoilers.peek().is_some_and(|&(_, end)| end < bar.position) {
            spoilers.next();
        }
        let in_spoiler = spoilers
            .peek()
            .is_some_and(|&(start, end)| start <= bar.position && bar.position <= end);
        if delimit && !bar.escaped && !in_spoiler {
            delimiters.push(bar.position);
        }
    }
    (delimiters, held)
}

/// The code spans of `text` as the inline parser reads them, each as the
/// offsets of its opening and closing backtick runs: a backtick run opens a
/// span that the next run of the same length closes, except that outside a
/// span an odd backslash run escapes the first backtick after it.
/// `may_close(open, close)` can refuse a pair, leaving the opener literal; it
/// is asked in increasing order of `open`.
fn inline_code_spans(
    text: &str,
    mut may_close: impl FnMut(usize, usize) -> bool,
) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    // Each backtick run's offset, length, and whether an odd backslash run
    // precedes it.
    let mut runs = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => {
                let backslashes = delimiter_byte_run_len(text, cursor, b'\\');
                cursor += backslashes;
                if bytes.get(cursor) == Some(&b'`') {
                    let length = delimiter_byte_run_len(text, cursor, b'`');
                    runs.push((cursor, length, backslashes % 2 == 1));
                    cursor += length;
                }
            }
            b'`' => {
                let length = delimiter_byte_run_len(text, cursor, b'`');
                runs.push((cursor, length, false));
                cursor += length;
            }
            _ => cursor += 1,
        }
    }
    let longest = runs.iter().map(|&(_, length, _)| length).max().unwrap_or(0);
    let mut by_length = vec![Vec::new(); longest + 1];
    for (index, &(_, length, _)) in runs.iter().enumerate() {
        by_length[length].push(index);
    }

    let mut spans = Vec::new();
    let mut index = 0;
    while index < runs.len() {
        let (position, length, escaped) = runs[index];
        let (open, length) = if escaped {
            (position + 1, length - 1)
        } else {
            (position, length)
        };
        let close = by_length[length]
            .get(by_length[length].partition_point(|&later| later <= index))
            .copied()
            .filter(|_| length > 0);
        match close {
            Some(close) if may_close(open, runs[close].0) => {
                spans.push((open, runs[close].0));
                index = close + 1;
            }
            _ => index += 1,
        }
    }
    spans
}

/// A pipe of a table row.
struct TableBar {
    position: usize,
    escaped: bool,
}

/// Adds the pipe at `position` to the bar runs, joining the previous run when
/// the cell text has the two side by side. An escaped pipe is an escape in the
/// cell's inline content, so it joins no run and no run joins it.
fn push_table_bar(
    bars: &mut Vec<TableBar>,
    runs: &mut Vec<(usize, usize)>,
    position: usize,
    text_start: usize,
    escaped: bool,
) {
    let joins = !escaped
        && bars
            .last()
            .is_some_and(|previous| !previous.escaped && previous.position + 1 == text_start);
    match runs.last_mut() {
        Some(run) if joins => run.1 += 1,
        _ => runs.push((bars.len(), 1)),
    }
    bars.push(TableBar { position, escaped });
}

/// The byte ranges of `input`'s cells, between the pipes that delimit them.
fn table_row_cell_ranges(input: &str, spoiler: bool) -> Vec<(usize, usize)> {
    let trimmed = input.trim_matches([' ', '\t']);
    let offset = leading_trim_bytes(input);
    let delimiters = table_row_delimiters(trimmed, spoiler);
    let mut cells = Vec::new();
    let mut start = 0;
    for &pipe in &delimiters {
        cells.push((offset + start, offset + pipe));
        start = pipe + 1;
    }
    cells.push((offset + start, offset + trimmed.len()));

    // A delimiter at the very start or end (only whitespace after it) is a
    // border, not the edge of an empty cell.
    if delimiters.first() == Some(&0) {
        cells.remove(0);
    }
    if delimiters
        .last()
        .is_some_and(|&pipe| is_blank(&trimmed[pipe + 1..]))
    {
        cells.pop();
    }
    cells
}

/// One table cell's inline input (whitespace trimmed, escaped pipes read as
/// `|`), its source map, and its span.
struct TableCellSource {
    text: DerivedText,
    /// The offsets in `text` of the `|`s read from `\|`.
    escaped_pipes: Vec<usize>,
    span: Span,
}

/// The cells of `row`, a slice of `line.text`, as `table_row_cell_ranges`
/// splits it.
fn table_row_cells(line: &Line<'_>, row: &str, spoiler: bool) -> Vec<TableCellSource> {
    let row_offset = slice_offset_in(line.text, row).unwrap_or(0);
    table_row_cell_ranges(row, spoiler)
        .into_iter()
        .map(|(start, end)| {
            let raw = &row[start..end];
            let leading = leading_trim_bytes(raw);
            let content = raw.trim_matches([' ', '\t']);
            let offset = row_offset + start + leading;
            let span = Span::new(
                line.source_start(offset),
                line.source_end(offset + content.len()),
            );
            let mut text = DerivedText::default();
            let mut escaped_pipes = Vec::new();
            // GitHub/cmark-gfm reads an odd backslash run before `|` as a
            // literal pipe; the run keeps its other backslashes, which the
            // inline parser resolves as written. The `|` read from `\|` maps
            // to both of its bytes.
            let bytes = content.as_bytes();
            let mut copied = 0;
            let mut cursor = 0;
            while cursor < bytes.len() {
                if bytes[cursor] == b'\\' {
                    let pipe = cursor + delimiter_byte_run_len(content, cursor, b'\\');
                    if bytes.get(pipe) == Some(&b'|') && (pipe - cursor) % 2 == 1 {
                        text.append(line, &content[copied..pipe - 1]);
                        escaped_pipes.push(text.text.len());
                        text.append_replacing(
                            "|",
                            line.source_start(offset + pipe - 1),
                            line.source_end(offset + pipe + 1),
                        );
                        copied = pipe + 1;
                    }
                    cursor = pipe;
                } else {
                    cursor += 1;
                }
            }
            text.append(line, &content[copied..]);
            TableCellSource {
                text,
                escaped_pipes,
                span,
            }
        })
        .collect()
}

fn table_has_separator(header: &str, delimiter: &str, spoiler: bool) -> bool {
    // GFM makes leading/trailing pipes optional, so `parse_table_delimiter` plus
    // the header/alignment column-count check usually suffice. The one exception
    // is a single resolved column with no disambiguating syntax: `a\n-\nb` has
    // matching one-column shapes yet no pipe and no alignment colon, so it is a
    // loose paragraph/setext, not a table. A single column still forms a table
    // when a pipe appears in the header/delimiter or the delimiter carries an
    // explicit alignment colon (`a\n-:`, `a\n:-:`, …).
    let Some(alignments) = parse_table_delimiter(delimiter, spoiler) else {
        return true;
    };
    if alignments.len() == 1 {
        return contains_unescaped_pipe(header, spoiler)
            || contains_unescaped_pipe(delimiter, spoiler)
            || delimiter.contains(':');
    }
    true
}

// Still used by `block_quote_table_body_row` to detect a table row appearing as
// a block-quote continuation line (which DOES require a pipe).
fn contains_unescaped_pipe(input: &str, spoiler: bool) -> bool {
    !table_row_delimiters(input, spoiler).is_empty()
}

// A GFM footnote definition `[^label]:` is a block boundary: it interrupts a
// paragraph and ends a prior footnote's lazy continuation.
/// The end of the `<…>` autolink opening at `index`.
fn autolink_end(lookups: &mut impl Lookups, input: &str, index: usize) -> Option<usize> {
    let end = lookups.find(">", index)? + 1;
    is_angle_autolink(&input[index + 1..end - 1]).then_some(end)
}

/// The end of the inline HTML (comment, processing instruction, CDATA,
/// declaration, or tag) opening at `index`.
fn html_inline_end(lookups: &mut impl Lookups, input: &str, index: usize) -> Option<usize> {
    let rest = &input[index..];
    if rest.starts_with("<!--") {
        return Some(lookups.find("-->", index)? + 3);
    }
    // The `?>` closing a processing instruction follows its `<?`.
    if rest.starts_with("<?") {
        return Some(lookups.find("?>", index + 2)? + 2);
    }
    if rest.starts_with("<![CDATA[") {
        return Some(lookups.find("]]>", index)? + 3);
    }
    if is_declaration_start(rest) {
        return Some(lookups.find(">", index)? + 1);
    }
    parse_html_tag_with(lookups, input, index).map(|(end, _)| end)
}

fn parse_html_tag(input: &str, index: usize) -> Option<(usize, &str)> {
    parse_html_tag_with(&mut DirectLookups { input }, input, index)
}

fn parse_html_tag_with<'a>(
    lookups: &mut impl Lookups,
    input: &'a str,
    index: usize,
) -> Option<(usize, &'a str)> {
    let bytes = input.as_bytes();
    if bytes.get(index) != Some(&b'<') {
        return None;
    }

    let closing = bytes.get(index + 1) == Some(&b'/');
    let name_start = index + if closing { 2 } else { 1 };
    let first = *bytes.get(name_start)?;
    if !first.is_ascii_alphabetic() {
        return None;
    }

    let mut cursor = name_start + 1;
    while bytes.get(cursor).is_some_and(|byte| html_name_byte(*byte)) {
        cursor += 1;
    }
    let name = &input[name_start..cursor];

    if closing {
        cursor = skip_spaces(input, cursor);
        if bytes.get(cursor) == Some(&b'>') {
            return Some((cursor + 1, name));
        }
        return None;
    }

    let mut needs_space = false;
    loop {
        let before_spaces = cursor;
        cursor = skip_spaces(input, cursor);
        let had_space = cursor > before_spaces;
        match bytes.get(cursor) {
            Some(b'>') => return Some((cursor + 1, name)),
            Some(b'/') if bytes.get(cursor + 1) == Some(&b'>') => return Some((cursor + 2, name)),
            Some(byte) if had_space && html_attribute_name_start(*byte) => {
                cursor += 1;
                while bytes
                    .get(cursor)
                    .is_some_and(|byte| html_attribute_name_byte(*byte))
                {
                    cursor += 1;
                }
                let after_name = cursor;
                let after_spaces = skip_spaces(input, cursor);
                if bytes.get(after_spaces) == Some(&b'=') {
                    cursor = skip_spaces(input, after_spaces + 1);
                    cursor = parse_html_attribute_value(lookups, input, cursor)?;
                } else {
                    cursor = after_name;
                }
                needs_space = true;
            }
            Some(_) if needs_space => return None,
            _ => return None,
        }
    }
}

fn parse_html_attribute_value(
    lookups: &mut impl Lookups,
    input: &str,
    index: usize,
) -> Option<usize> {
    let bytes = input.as_bytes();
    match bytes.get(index)? {
        b'"' => Some(lookups.find("\"", index + 1)? + 1),
        b'\'' => Some(lookups.find("'", index + 1)? + 1),
        b'=' | b'<' | b'>' | b'`' => None,
        _ => {
            let mut cursor = index;
            while bytes.get(cursor).is_some_and(|byte| {
                !byte.is_ascii_whitespace()
                    && !matches!(*byte, b'"' | b'\'' | b'=' | b'<' | b'>' | b'`')
            }) {
                cursor += 1;
            }
            if cursor == index {
                None
            } else {
                Some(cursor)
            }
        }
    }
}

fn html_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-'
}

fn html_attribute_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b':'
}

fn html_attribute_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':' | b'.' | b'-')
}

fn skip_spaces(input: &str, mut index: usize) -> usize {
    while input
        .as_bytes()
        .get(index)
        .is_some_and(|byte| matches!(*byte, b' ' | b'\t' | b'\n' | b'\r'))
    {
        index += 1;
    }
    index
}

/// The destination of the angle-bracket autolink `<uri>`, when `uri` writes
/// one: the URI itself, or `mailto:` and an email address.
pub(crate) fn angle_autolink_destination(uri: &str) -> Option<String> {
    if is_uri_autolink(uri) {
        Some(String::from(uri))
    } else if is_email_autolink(uri) {
        Some(alloc::format!("mailto:{uri}"))
    } else {
        None
    }
}

/// Whether `<uri>` is an angle-bracket autolink.
fn is_angle_autolink(uri: &str) -> bool {
    is_uri_autolink(uri) || is_email_autolink(uri)
}

fn is_uri_autolink(input: &str) -> bool {
    // A scheme is at most 32 bytes, so the `:` ending it is within the first 33.
    let Some(colon) = input
        .as_bytes()
        .iter()
        .take(33)
        .position(|byte| *byte == b':')
    else {
        return false;
    };
    let scheme = &input[..colon];
    if scheme.len() < 2 || scheme.len() > 32 {
        return false;
    }
    let mut bytes = scheme.bytes();
    if !bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic()) {
        return false;
    }
    if !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-')) {
        return false;
    }
    input[colon + 1..]
        .chars()
        .map(source_char)
        .all(|char| !matches!(char, '<' | '>' | ' ') && !char.is_ascii_control())
}

fn is_email_autolink(input: &str) -> bool {
    // No email address contains `<`, and stopping at one keeps a run of `<`
    // openers that share one later `>` linear.
    if input
        .chars()
        .any(|char| char.is_whitespace() || char == '<')
    {
        return false;
    }
    let Some(at) = input.find('@') else {
        return false;
    };
    if at == 0 || at + 1 >= input.len() {
        return false;
    }
    // Angle-bracket `<email>` autolinks use the strict CommonMark domain
    // grammar but, unlike the GFM bare form, allow a single (dotless) label.
    is_email_local_part(&input[..at]) && is_email_domain(&input[at + 1..], 1)
}

// GFM literal-autolink dispatch. Tries, in order: `http(s)://` URLs, `www.`
// URLs, extended-protocol (`mailto:`/`xmpp:`) emails, and bare emails. Each
// branch enforces cmark-gfm's per-scheme preceding-character guard and its
// domain/host rules; the trailing trim is shared (`autolink_delim`). The
// returned destination is the synthesized href (a `http://`/`mailto:` prefix
// may be prepended); the caller keeps `input[index..end]` as the visible
// original.
fn wikilinks_enabled(options: &SyntaxOptions) -> bool {
    options.constructs.wikilink_title_after_pipe || options.constructs.wikilink_title_before_pipe
}

fn parse_literal_autolink(
    input: &str,
    index: usize,
    gfm: bool,
    relaxed: bool,
    wikilinks: bool,
    scan: &mut LiteralAutolinkScan,
) -> Option<(usize, String)> {
    let rest = &input[index..];

    if gfm {
        // `http://` / `https://` URLs. cmark requires the char before the scheme
        // to be non-alphanumeric (so `mmmhttp://…` does not link from `mmmh`).
        if let Some(scheme_len) = rest
            .starts_with("http://")
            .then_some(7)
            .or_else(|| rest.starts_with("https://").then_some(8))
        {
            if !literal_scheme_prefix_ok(input, index) {
                return None;
            }
            let host = &input[index + scheme_len..];
            // A non-empty domain or bracketed IPv6 host is additionally
            // required, so `http://`, `http://#`, `http://$` are not links.
            if !http_literal_host_ok(host) {
                if relaxed {
                    // Let cmark-gfm's relaxed `scheme://` pass decide cases
                    // such as a bare `http://` followed by whitespace.
                } else {
                    return None;
                }
            } else {
                // The URL extent is scanned from the very start (after `://`) and the
                // trailing trim runs over the whole URL. Relaxed mode balances
                // brackets/braces so `[abc]`/`{abc}`/IPv6 hosts stay in the URL.
                let end = autolink_url_end(
                    input,
                    index + scheme_len,
                    index + scheme_len,
                    relaxed,
                    wikilinks,
                );
                if end <= index + scheme_len {
                    return None;
                }
                if literal_autolink_suppressed_by_link_label(
                    input,
                    index,
                    end,
                    relaxed,
                    gfm,
                    &mut scan.label_openers,
                ) {
                    return None;
                }
                return Some((end, input[index..end].into()));
            }
        }

        // `www.` URLs (synthesize a `http://` href). cmark allows the preceding
        // char to be one of `*_~(` or whitespace (or start of input).
        if rest
            .as_bytes()
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"www."))
        {
            if !literal_www_prefix_ok(input, index) {
                return None;
            }
            check_domain(rest, false)?;
            let end = autolink_url_end(input, index, index, relaxed, wikilinks);
            if end <= index || (!relaxed && end <= index + 3 && !literal_starts_line(input, index))
            {
                return None;
            }
            if literal_autolink_suppressed_by_link_label(
                input,
                index,
                end,
                relaxed,
                gfm,
                &mut scan.label_openers,
            ) {
                return None;
            }
            let mut destination = String::from("http://");
            destination.push_str(&input[index..end]);
            return Some((end, destination));
        }

        if let Some(email) = parse_literal_email(input, index, &mut scan.email_local) {
            return Some(email);
        }
    }

    if relaxed {
        // cmark-gfm "relaxed" URL autolinks: a bare `scheme://…` for any scheme
        // (`smb://`, `irc://`, `rdar://`, `we://`, `nex://[…]`, …) or a
        // scheme-less leading `://…` (`://-`). Requires the same non-alphanumeric
        // preceding char as the http literal; a scheme-less `://` also needs a
        // char other than whitespace after it, while a named scheme links on
        // its own (`https:// x` links `https://`, as cmark-gfm does). No
        // host/domain validation (cmark-gfm is permissive here — `smb:///path`
        // and `://-` both linkify). The extent is balanced.
        if literal_scheme_prefix_ok(input, index) {
            if let Some(after_slashes) =
                relaxed_scheme_after_slashes(input, index, &mut scan.scheme)
            {
                let body_start = index + after_slashes;
                let next = input[body_start..].chars().next();
                if next.is_none_or(|char| char.is_whitespace()) && after_slashes == 3 {
                    return None;
                }
                let end = autolink_url_end(input, body_start, body_start, true, wikilinks);
                if end > index {
                    if literal_autolink_suppressed_by_link_label(
                        input,
                        index,
                        end,
                        relaxed,
                        gfm,
                        &mut scan.label_openers,
                    ) {
                        return None;
                    }
                    return Some((end, input[index..end].into()));
                }
            }
        }
    }

    None
}

// Returns the byte offset (within `rest`) just past a relaxed `scheme://` (any
// ASCII-alpha-then-`[alnum+. -]` scheme) or scheme-less `://` prefix, if `rest`
// starts with one. No scheme length cap — cmark-gfm's relaxed autolink is
// permissive. Returns `None` for a bare `scheme:` without `//` (that is the
// email/angle-autolink path's job).
fn relaxed_scheme_after_slashes(
    input: &str,
    index: usize,
    scheme_run: &mut ByteRun,
) -> Option<usize> {
    let bytes = input[index..].as_bytes();
    if bytes.starts_with(b"://") {
        return Some(3);
    }
    let first = bytes.first()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    // The scheme is the run of scheme bytes starting here, and `://` must
    // follow it directly.
    let i = scheme_run.end(input, index, is_relaxed_scheme_byte) - index;
    if bytes.get(i..i + 3) == Some(b"://") {
        Some(i + 3)
    } else {
        None
    }
}

// The char immediately before a `http(s)://` literal must be non-alphabetic.
// An escaped `<` (`\<http://…`) is just literal text before the URL, so the
// literal still forms (the `<` is not treated as an angle-autolink opener).
fn literal_scheme_prefix_ok(input: &str, index: usize) -> bool {
    if index == 0 {
        return true;
    }
    let Some(previous) = input[..index].chars().next_back() else {
        return true;
    };
    !previous.is_ascii_alphabetic()
}

// The char before a `www.` literal must be one of cmark-gfm's accepted ASCII
// delimiters or ordinary Markdown layout whitespace. As on GitHub, other
// Unicode whitespace is not a start delimiter for this branch.
fn literal_www_prefix_ok(input: &str, index: usize) -> bool {
    if index == 0 {
        return true;
    }
    let Some(previous) = input[..index].chars().next_back() else {
        return true;
    };
    if matches!(previous, '*' | '_' | '~' | '(' | '[' | ']') {
        return true;
    }
    matches!(previous, ' ' | '\t' | '\n' | '\r')
}

fn literal_starts_line(input: &str, index: usize) -> bool {
    index == 0
        || input
            .as_bytes()
            .get(index - 1)
            .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
}

fn literal_autolink_suppressed_by_link_label(
    input: &str,
    index: usize,
    end: usize,
    relaxed: bool,
    gfm_autolink_literal: bool,
    label_openers: &mut LabelOpenerScan,
) -> bool {
    if !label_openers.has_unclosed_opener(input, index) {
        return false;
    }
    if input[end..].starts_with("](") && !link_resource_tail_has_close(input, end + 2) {
        return true;
    }
    !relaxed && !gfm_autolink_literal && input.as_bytes().get(end).is_some_and(|byte| *byte == b']')
}

/// Whether an unclosed `[` precedes a literal autolink on its line: the
/// `[`/`]` depth from the line start (honouring `\` escapes) is positive. The
/// walk is kept across calls because one inline scan asks about increasing
/// positions; it drops back to depth 0 after each line break it consumes, which
/// is exactly the state a walk from that line's start reaches.
#[derive(Default)]
struct LabelOpenerScan {
    /// The last position asked about; the walk has stopped at the first step
    /// at or after it.
    queried: usize,
    cursor: usize,
    depth: usize,
}

impl LabelOpenerScan {
    fn has_unclosed_opener(&mut self, input: &str, index: usize) -> bool {
        if index < self.queried {
            *self = Self::default();
        }
        while self.cursor < index {
            let Some((next, char)) = next_char(input, self.cursor) else {
                break;
            };
            match char {
                '\\' => {
                    // An escaped line break still ends the line, so it is left
                    // for the next step to consume as a break.
                    self.cursor = match next_char(input, next) {
                        Some((_, '\n' | '\r')) | None => next,
                        Some((after_escape, _)) => after_escape,
                    };
                    continue;
                }
                '\n' | '\r' => self.depth = 0,
                '[' => self.depth += 1,
                ']' => self.depth = self.depth.saturating_sub(1),
                _ => {}
            }
            self.cursor = next;
        }
        self.queried = index;
        self.depth > 0
    }
}

fn link_resource_tail_has_close(input: &str, start: usize) -> bool {
    let mut cursor = start;
    while cursor < input.len() {
        let Some((next, char)) = next_char(input, cursor) else {
            break;
        };
        match char {
            '\\' => {
                cursor = next_char(input, next)
                    .map(|(after_escape, _)| after_escape)
                    .unwrap_or(next);
                continue;
            }
            '\n' | '\r' => return false,
            ')' => return true,
            _ => {}
        }
        cursor = next;
    }
    false
}

fn http_literal_host_ok(host: &str) -> bool {
    if host.starts_with('[') {
        return bracketed_ipv6_host_end(host).is_some();
    }
    match host.chars().next() {
        Some(char) if char.is_ascii() && char.is_ascii_alphanumeric() => {
            check_domain(host, true).is_some()
        }
        Some(char) if !char.is_ascii() && is_valid_hostchar(source_char(char)) => {
            check_domain(host, true).is_some()
        }
        _ => false,
    }
}

fn bracketed_ipv6_host_end(host: &str) -> Option<usize> {
    let close = host.find(']')?;
    (close > 1).then_some(close + 1)
}

// Port of cmark-gfm `is_valid_hostchar`: a host char is valid when it is not a
// Unicode space and not a Unicode punctuation character.
fn is_valid_hostchar(char: char) -> bool {
    !char.is_whitespace() && !crate::unicode_punctuation::is_unicode_punctuation(char)
}

// Port of cmark-gfm `check_domain`. Scans the leading host of `data` (up to the
// first non-host char) and returns its byte length, or `None` when invalid.
// Rejects a `_` in either of the last two `.`-separated host segments (unless
// the host has >10 segments — a DoS guard). When `allow_short` is false a dot
// is required (the `www.` rule). The URL extent past the host is determined by
// `autolink_url_end`, so the precise length here only gates validity.
//
// cmark walks bytes with `is_valid_hostchar` decoding each char; this walks
// chars directly (UTF-8 safe) over the host prefix, which yields the same
// dot/underscore-segment verdict. A `\` escapes the following char.
fn check_domain(data: &str, allow_short: bool) -> Option<usize> {
    let mut np = 0usize;
    let mut uscore1 = 0usize;
    let mut uscore2 = 0usize;
    let mut host_len = 0usize;

    let mut chars = data.char_indices().peekable();
    while let Some((offset, char)) = chars.next() {
        // cmark's accounting loop runs `for (i = 1; i < size - 1; i++)`: it
        // never inspects the first char (offset 0) nor the final char of the
        // chunk. We replicate that — a trailing `_` (e.g. `http://a_`) is not
        // counted, so the link still forms.
        let account = offset != 0 && chars.peek().is_some();
        match char {
            '\\' => {
                // Escape: consume the next char as a literal host char.
                host_len = offset + char.len_utf8();
                if let Some((next_off, next)) = chars.next() {
                    host_len = next_off + next.len_utf8();
                }
            }
            '_' if account => {
                uscore2 += 1;
                host_len = offset + char.len_utf8();
            }
            '.' if account => {
                uscore1 = uscore2;
                uscore2 = 0;
                np += 1;
                host_len = offset + char.len_utf8();
            }
            '_' | '.' | '-' => {
                host_len = offset + char.len_utf8();
            }
            _ => {
                if !is_valid_hostchar(source_char(char)) {
                    break;
                }
                host_len = offset + char.len_utf8();
            }
        }
    }

    if (uscore1 > 0 || uscore2 > 0) && np <= 10 {
        return None;
    }

    if allow_short || np > 0 {
        Some(host_len)
    } else {
        None
    }
}

// Forward scan from `start` for the URL extent: Unicode whitespace, `<`, a
// non-ASCII char in CommonMark's Unicode punctuation set (full-width `，` or
// `。`) other than the replacement char, `[[` with wikilinks enabled, or `]`
// ends the URL. CommonMark allows
// `>` and `[` inside (the renderer percent-encodes them); a `]` is
// additionally treated as a hard URL boundary (autolink-3), so a `]` ends the
// scan and is never part of the link. `trim_from` is where the trailing trim
// may reach (the URL start).
fn autolink_url_end(
    input: &str,
    start: usize,
    trim_from: usize,
    balanced: bool,
    wikilinks: bool,
) -> usize {
    let bytes = input.as_bytes();
    let mut end = start;
    // Relaxed (cmark-gfm) URL extents balance `[`/`]` and `{`/`}` so an IPv6
    // host `nex://[fe80…]/z` and a balanced `[abc]`/`{abc}` run stay inside the
    // URL while an unbalanced trailing `]`/`}` ends it. Strict (GFM literal)
    // extents stop at the first `]` (no balancing) — the two oracle shapes
    // differ on purpose (`autolink_brackets_unbalanced` keeps both `]`;
    // `autolink_relaxed_links_brackets_balanced` keeps one).
    let mut bracket_depth = 0i32;
    let mut curly_depth = 0i32;
    let mut strict_has_open_bracket = false;
    let mut strict_inside_backticks = false;
    for (offset, char) in input[start..].char_indices() {
        if char.is_whitespace()
            || char == '<'
            || is_autolink_terminating_control(char)
            || (!char.is_ascii()
                && char != '\u{FFFD}'
                && crate::unicode_punctuation::is_unicode_punctuation(char))
            || (wikilinks && input[start + offset..].starts_with("[["))
        {
            break;
        }
        if balanced {
            match char {
                '[' => bracket_depth += 1,
                ']' => {
                    if bracket_depth > 0 {
                        bracket_depth -= 1;
                    } else {
                        break;
                    }
                }
                '{' => curly_depth += 1,
                '}' => {
                    if curly_depth > 0 {
                        curly_depth -= 1;
                    } else {
                        break;
                    }
                }
                _ => {}
            }
        } else {
            match char {
                '[' => strict_has_open_bracket = true,
                '`' => strict_inside_backticks = !strict_inside_backticks,
                ']' if !strict_has_open_bracket && !strict_inside_backticks => break,
                _ => {}
            }
        }
        // A `\` before ASCII punctuation other than `.` ends the URL. The
        // serializer writes text after a literal autolink with backslash
        // escapes (`\*`, `\_`, `\[`, …); stopping here keeps such a tree
        // writable. cmark-gfm keeps the `\*x` in `www.a.com\*x` inside the
        // URL; this crate deliberately diverges. A `\` before `.` or a
        // non-punctuation char stays part of the URL (`www.x.com/a\.`).
        if char == '\\' {
            if let Some(&next) = bytes.get(start + offset + 1) {
                if next.is_ascii_punctuation() && next != b'.' {
                    break;
                }
            }
        }
        end = start + offset + char.len_utf8();
    }
    autolink_delim(input, trim_from, end)
}

fn is_autolink_terminating_control(char: char) -> bool {
    matches!(char, '\u{2066}'..='\u{2069}')
}

// Port of cmark-gfm `autolink_delim`: trim trailing delimiters from the end of
// the URL. A trailing `) ? ! . , : * _ ~ ' "` is trimmed; `)` only when there
// are more `)` than `(` in the link; a trailing `&…;` entity run is excluded
// whole; a lone trailing `;` is trimmed.
fn autolink_delim(input: &str, start: usize, mut end: usize) -> usize {
    let bytes = input.as_bytes();
    let mut opening = 0usize;
    let mut closing = 0usize;
    for &byte in &bytes[start..end] {
        match byte {
            b'(' => opening += 1,
            b')' => closing += 1,
            _ => {}
        }
    }

    while end > start {
        match bytes[end - 1] {
            b')' => {
                if closing <= opening {
                    break;
                }
                closing -= 1;
                end -= 1;
            }
            b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"' => {
                end -= 1;
            }
            b';' => {
                // A trailing hex numeric character reference `&#x…;` is excluded
                // whole. This is the round-trip dual of the serializer, which
                // encodes a text char that would otherwise merge into the URL as
                // a hex entity; no autolink-oracle URL ends in `&#x…;`, so this
                // is conformance-safe (decimal `&#…;` is left intact to match
                // the oracle, which keeps `www.a&#35` in the URL).
                if let Some(amp) = trailing_hex_entity_run_start(bytes, start, end) {
                    end = amp;
                } else {
                    // Walk back over alphanumerics; if they reach a `&`, exclude
                    // the whole `&…;` entity run, otherwise trim just the `;`.
                    let mut new_end = end - 1;
                    while new_end > start && bytes[new_end - 1].is_ascii_alphanumeric() {
                        new_end -= 1;
                    }
                    if new_end > start && new_end < end - 1 && bytes[new_end - 1] == b'&' {
                        end = new_end - 1;
                    } else {
                        end -= 1;
                    }
                }
            }
            _ => break,
        }
    }
    end
}

// When the URL ends with a hex numeric character reference `&#x[hex]+;`, returns
// the offset of its leading `&`; otherwise `None`. Used only by `autolink_delim`
// to trim the serializer's round-trip boundary marker (the serializer encodes a
// would-merge text char as `&#xNN;`). Decimal `&#…;` is intentionally NOT
// matched so the oracle's `www.a&#35` URLs stay intact.
fn trailing_hex_entity_run_start(bytes: &[u8], start: usize, end: usize) -> Option<usize> {
    if end <= start || bytes[end - 1] != b';' {
        return None;
    }
    let mut cursor = end - 1;
    while cursor > start && bytes[cursor - 1].is_ascii_hexdigit() {
        cursor -= 1;
    }
    // Require at least one hex digit, then `&#x` (case-insensitive `x`).
    if cursor == end - 1 || cursor < start + 3 {
        return None;
    }
    let x = bytes[cursor - 1];
    if (x == b'x' || x == b'X') && bytes[cursor - 2] == b'#' && bytes[cursor - 3] == b'&' {
        Some(cursor - 3)
    } else {
        None
    }
}

// GFM bare-email literal (and the extended `mailto:`/`xmpp:` protocol forms).
// `index` must be the link start: cmark anchors the email at the left edge
// found by rewinding from `@` over `[A-Za-z0-9._+-]` (or a `mailto:`/`xmpp:`
// scheme), so this only succeeds when the char before `index` is not part of
// that left extent.
/// Scan state shared by the `parse_literal_autolink` calls of one inline pass.
/// The pass only moves forward, so each piece lets a later start position reuse
/// the bytes already walked for an earlier one instead of rescanning them.
#[derive(Default)]
struct LiteralAutolinkScan {
    /// Runs of email local-part bytes (plus `:` for `mailto:`/`xmpp:`).
    email_local: ByteRun,
    /// Runs of relaxed-autolink scheme bytes.
    scheme: ByteRun,
    label_openers: LabelOpenerScan,
}

/// The extent of one run of bytes accepted by a fixed predicate: every start
/// position in `start..=end` stops at `end`. Each `ByteRun` is always queried
/// with the same predicate.
#[derive(Default)]
struct ByteRun {
    start: usize,
    end: Option<usize>,
}

impl ByteRun {
    /// Returns the first byte position at or after `index` that `in_run`
    /// rejects, or the input length.
    fn end(&mut self, input: &str, index: usize, in_run: fn(u8) -> bool) -> usize {
        if let Some(end) = self.end {
            if self.start <= index && index <= end {
                return end;
            }
        }
        let end = input.as_bytes()[index..]
            .iter()
            .position(|byte| !in_run(*byte))
            .map_or(input.len(), |offset| index + offset);
        self.start = index;
        self.end = Some(end);
        end
    }
}

fn is_email_local_or_scheme_byte(byte: u8) -> bool {
    is_gfm_email_local_byte(byte) || byte == b':'
}

fn is_relaxed_scheme_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-')
}

fn parse_literal_email(
    input: &str,
    index: usize,
    local_run: &mut ByteRun,
) -> Option<(usize, String)> {
    let rest = &input[index..];
    // A valid local part holds only local-part bytes behind an optional
    // `mailto:`/`xmpp:` scheme, so the `@` must be the byte that ends that run.
    let at_index = local_run.end(input, index, is_email_local_or_scheme_byte);
    if input.as_bytes().get(at_index) != Some(&b'@') {
        return None;
    }
    let at = at_index - index;
    if at == 0 {
        return None;
    }
    let local = &rest[..at];

    // Determine whether this `@` is preceded by an extended protocol scheme
    // (`mailto:` / `xmpp:`), which both relaxes the href synthesis and (xmpp)
    // allows `/` in the domain.
    let (auto_mailto, is_xmpp) = classify_email_local(local);

    // Left-boundary guard (autolink-1): the char before `index` must not be a
    // local-part continuation char, otherwise the true link starts earlier and
    // this position is interior. After a recognized scheme, the scheme's own
    // preceding-char rule is what matters.
    if !email_left_boundary_ok(input, index, auto_mailto) {
        return None;
    }

    if !email_local_is_valid(local, auto_mailto) {
        return None;
    }

    let domain_start = index + at + 1;
    let domain_end = literal_email_domain_end(input, domain_start, is_xmpp)?;
    let trimmed = autolink_delim(input, domain_start, domain_end);
    if trimmed <= domain_start {
        return None;
    }

    let domain = &input[domain_start..trimmed];
    if !is_gfm_email_domain(domain, is_xmpp) {
        return None;
    }

    let mut destination = String::new();
    if auto_mailto {
        destination.push_str("mailto:");
    }
    destination.push_str(&input[index..trimmed]);
    Some((trimmed, destination))
}

// Classify the local part for the extended-protocol forms. Returns
// `(auto_mailto, is_xmpp)`: `mailto:user` → (false, false); `xmpp:user` →
// (false, true); a bare local part → (true, false). The scheme match is
// case-insensitive.
fn classify_email_local(local: &str) -> (bool, bool) {
    if let Some(rest) = strip_ci_prefix(local, "mailto:") {
        if !rest.is_empty() {
            return (false, false);
        }
    }
    if let Some(rest) = strip_ci_prefix(local, "xmpp:") {
        if !rest.is_empty() {
            return (false, true);
        }
    }
    (true, false)
}

fn strip_ci_prefix<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    let bytes = input.as_bytes();
    let plen = prefix.len();
    if bytes.len() >= plen && bytes[..plen].eq_ignore_ascii_case(prefix.as_bytes()) {
        Some(&input[plen..])
    } else {
        None
    }
}

// The left-boundary check for an email literal. The link is anchored at its
// true left edge: the preceding char must not be an ASCII alphanumeric (which
// would extend the local part leftward). For the bare form, a preceding `/` is
// also rejected (`/a@b.c` is not linked), while the extended
// `mailto:`/`xmpp:` form permits `/` before the scheme (so
// `…/mailto:beedrill@…` links).
fn email_left_boundary_ok(input: &str, index: usize, auto_mailto: bool) -> bool {
    if index == 0 {
        return true;
    }
    let Some(previous) = input[..index].chars().next_back() else {
        return true;
    };
    if previous.is_ascii_alphanumeric() {
        if auto_mailto
            && input[index..].starts_with('+')
            && prefix_ends_with_gfm_email(input, index)
        {
            return true;
        }
        return false;
    }
    if auto_mailto && previous == '/' {
        return false;
    }
    true
}

fn prefix_ends_with_gfm_email(input: &str, end: usize) -> bool {
    let start = input[..end]
        .char_indices()
        .rev()
        .find(|(_, char)| char.is_whitespace())
        .map_or(0, |(offset, char)| offset + char.len_utf8());
    let candidate = &input[start..end];
    let Some(at) = candidate.rfind('@') else {
        return false;
    };
    email_local_is_valid(&candidate[..at], true) && is_gfm_email_domain(&candidate[at + 1..], false)
}

// Validate the email local part. For the bare form, every char must be a GFM
// email atext byte (`[A-Za-z0-9.+_-]` plus the dot-separated structure). For
// the extended-protocol forms, the part after the scheme is validated.
fn email_local_is_valid(local: &str, auto_mailto: bool) -> bool {
    let body = if auto_mailto {
        local
    } else if let Some(rest) = strip_ci_prefix(local, "mailto:") {
        rest
    } else if let Some(rest) = strip_ci_prefix(local, "xmpp:") {
        rest
    } else {
        local
    };
    !body.is_empty() && body.bytes().all(is_gfm_email_local_byte)
}

// GFM email local-part charset (autolink-1): a narrower set than RFC atext,
// matching cmark's rewind class `[A-Za-z0-9.+_-]`.
fn is_gfm_email_local_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'_' | b'-')
}

fn is_email_local_part(input: &str) -> bool {
    !input.is_empty()
        && input
            .split('.')
            .all(|segment| !segment.is_empty() && segment.bytes().all(is_email_atext))
}

fn is_email_atext(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'/'
                | b'='
                | b'?'
                | b'^'
                | b'_'
                | b'`'
                | b'{'
                | b'|'
                | b'}'
                | b'~'
                | b'-'
        )
}

// Port of cmark-gfm's email-domain scan (`postprocess_text`). Scans forward
// from `index` over the email domain, accepting alphanumerics, `-`, `_`, and
// `.`; for the `xmpp:` form a `/` is also accepted (path). A dot only counts
// toward the "at least one dot" requirement when it is followed by an
// alphanumeric. The scanned span must be >= 1 byte, contain at least one such
// dot, and end in an alphabetic char or a dot. Returns the domain end offset
// (before trailing trim), or `None` when invalid.
fn literal_email_domain_end(input: &str, index: usize, is_xmpp: bool) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut end = index;
    let mut np = 0usize;
    while end < bytes.len() {
        let byte = bytes[end];
        if byte.is_ascii_alphanumeric() {
            end += 1;
        } else if byte == b'.' && end + 1 < bytes.len() && bytes[end + 1].is_ascii_alphanumeric() {
            np += 1;
            end += 1;
        } else if byte == b'-' || byte == b'_' || (byte == b'/' && is_xmpp) {
            // `-`/`_` always continue the domain; `/` continues only the xmpp
            // path form.
            end += 1;
        } else {
            break;
        }
    }
    if end <= index {
        return None;
    }
    let len = end - index;
    let last = bytes[end - 1];
    if len < 1 || np == 0 || !(last.is_ascii_alphabetic() || last == b'.') {
        return None;
    }
    Some(end)
}

// Final structural validation of the trimmed email domain. The cmark scan
// already enforced the dot/last-char rules; this re-checks them after the
// shared trailing trim removed any delimiters, and rejects a domain ending in
// `-`/`_` (autolink-7: a hyphen in the final label disqualifies the link).
fn is_gfm_email_domain(input: &str, is_xmpp: bool) -> bool {
    if input.is_empty() {
        return false;
    }
    // A `/` path is only legal in the `xmpp:` form; split it off for the host
    // structural checks.
    let host = if is_xmpp {
        input.split('/').next().unwrap_or(input)
    } else {
        input
    };
    if !host.contains('.') {
        return false;
    }
    let last = host.as_bytes()[host.len() - 1];
    // The final label must not end in `-` or `_`, and the trailing label may
    // not be all ASCII digits.
    if matches!(last, b'-' | b'_') {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    })
}

fn is_email_domain(input: &str, min_labels: usize) -> bool {
    let mut label_count = 0usize;
    for label in input.split('.') {
        label_count += 1;
        let bytes = label.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 63
            || !bytes
                .first()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
            || !bytes
                .last()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
            || !bytes
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        {
            return false;
        }
    }
    label_count >= min_labels
}

/// A footnote label: no space, tab, or line ending and, as in a link label,
/// no unescaped bracket.
fn is_footnote_label(label: &str) -> bool {
    !label.is_empty()
        && reference_label_is_within_limit(label)
        && !label.contains([' ', '\t', '\n', '\r'])
        && !label
            .match_indices(['[', ']'])
            .any(|(index, _)| !is_escaped_at(label, index))
}

fn find_footnote_definition_label_end(input: &str) -> Option<usize> {
    let close = find_footnote_reference_label_end(input, 2)?;
    if input.as_bytes().get(close + 1) == Some(&b':') {
        Some(close)
    } else {
        None
    }
}

fn find_footnote_reference_label_end(input: &str, mut cursor: usize) -> Option<usize> {
    while cursor < input.len() {
        let (next, char) = next_char(input, cursor)?;
        if char == ']' && !is_escaped_at(input, cursor) {
            return Some(cursor);
        }
        cursor = next;
    }
    None
}
