//! AST to canonical Markdown. The verbs live on [`Document`]
//! ([`to_markdown`](Document::to_markdown) /
//! [`to_markdown_with`](Document::to_markdown_with)); [`SerializeOptions`] tunes
//! the output style. The document is validated first, so serialization can fail
//! with a [`SerializeError`].

use alloc::{
    borrow::Cow,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use crate::{
    ast::*,
    diagnostic::Diagnostic,
    memo::{pattern_starts, PathMemo, Positions, Step},
    parse::{
        continuation_line_breaks_paragraph, gfm_table_can_start_source, is_flanking_punctuation,
        line_starts_html_block, line_starts_interrupting_html_block, literal_autolink_extents,
    },
    validate::{is_directive_name, validate_document},
};

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

/// Output-style options for serialization. Defaults: LF, trailing newline, `-`
/// bullets, `.` ordered markers, and backtick code fences.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SerializeOptions {
    /// Newline style to emit.
    pub line_ending: LineEnding,
    /// Whether to end the output with a trailing newline.
    pub final_newline: bool,
    /// The bullet marker for unordered lists.
    pub bullet: ListDelimiter,
    /// The delimiter for ordered-list markers (e.g. `.` → `1.`).
    pub ordered_delimiter: ListDelimiter,
    /// The fence character for fenced code blocks.
    pub fence_marker: FenceMarker,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            line_ending: LineEnding::Lf,
            final_newline: true,
            bullet: ListDelimiter::Dash,
            ordered_delimiter: ListDelimiter::Period,
            fence_marker: FenceMarker::Backtick,
        }
    }
}

/// Why serialization failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SerializeError {
    /// The AST failed validation; carries the validation diagnostics.
    InvalidDocument(Vec<Diagnostic>),
    /// A node kind that the serializer does not support was encountered.
    UnsupportedNode(&'static str),
}

impl Document {
    /// Serialize this document to canonical Markdown with default options.
    pub fn to_markdown(&self) -> Result<String, SerializeError> {
        self.to_markdown_with(&SerializeOptions::default())
    }

    /// Serialize this document to canonical Markdown with explicit options.
    pub fn to_markdown_with(&self, options: &SerializeOptions) -> Result<String, SerializeError> {
        let diagnostics = validate_document(self);
        if !diagnostics.is_empty() {
            return Err(SerializeError::InvalidDocument(diagnostics));
        }

        serialize_document_body(self, options)
    }
}

fn serialize_document_body(
    document: &Document,
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
    let mut output = serialize_blocks_at_start(&document.children, options, true)?;
    if options.line_ending == LineEnding::CrLf {
        output = lf_to_crlf(&output);
    }
    // Blocks end without a line ending, except an HTML block whose last line
    // is empty (the final newline ends that line too) and an indented code
    // block that keeps its value's `\r` or `\r\n` ending.
    if options.final_newline && !output.is_empty() && !ends_with_carriage_return_ending(&output) {
        output.push_str(options.line_ending.as_str());
    }
    Ok(output)
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

/// Serialize a block sequence. `document_start` is true only for the top-level
/// document body, where the first block sits at byte 0 and a contiguous `---`
/// would open frontmatter; that one position emits a spaced dash thematic break
/// instead. Nested sequences (blockquotes, list items, ...) are never at byte 0.
fn serialize_blocks_at_start(
    blocks: &[Block],
    options: &SerializeOptions,
    document_start: bool,
) -> Result<String, SerializeError> {
    // Written last to first: a list reads the indentation of the block after
    // it, which would join its last item unless the items' content starts
    // further in.
    let mut outputs: Vec<String> = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.iter().enumerate().rev() {
        let at_document_start = document_start && index == 0;
        let next_indent = outputs
            .last()
            .map(|next: &String| next.len() - next.trim_start_matches(' ').len());
        let written = match (block, next_indent) {
            (Block::List(list), Some(indent @ 1..=3)) => {
                serialize_list_with_marker_spacing(list, options, &" ".repeat(indent), " ")?
            }
            (Block::List(list), Some(4..)) => {
                serialize_list_with_marker_spacing(list, options, " ", "    ")?
            }
            _ => serialize_block(block, options, at_document_start)?,
        };
        outputs.push(written);
    }
    let mut output = String::new();
    for (index, written) in outputs.iter().rev().enumerate() {
        if index > 0 {
            let first_line = written.split('\n').next().unwrap_or("");
            let opens_with_html = || match &blocks[index] {
                Block::Paragraph(paragraph) => paragraph.children.first(),
                Block::Heading(heading) if heading.kind == HeadingKind::Setext => {
                    heading.children.first()
                }
                _ => None,
            };
            let continues_definition = matches!(blocks[index - 1], Block::Definition(_))
                && matches!(opens_with_html(), Some(Inline::Html(_)))
                && line_starts_html_block(first_line);
            if continues_definition {
                // Raw HTML that would start an HTML block opens such content
                // only as the continuation of the paragraph a definition was
                // read from; a tag that interrupts it is indented as well.
                output.push('\n');
                if line_starts_interrupting_html_block(first_line) {
                    output.push_str("    ");
                }
            } else {
                push_block_gap(&mut output);
            }
        }
        output.push_str(written);
    }
    Ok(output)
}

fn serialize_block(
    block: &Block,
    options: &SerializeOptions,
    at_document_start: bool,
) -> Result<String, SerializeError> {
    match block {
        Block::Paragraph(node) => serialize_paragraph(node, options),
        Block::Heading(node) => {
            let content = serialize_inlines(&node.children, options)?;
            // A setext underline can only express depth 1 (`=`) or 2 (`-`); any
            // other depth must fall back to ATX, otherwise the depth is lost.
            // Multi-line content stays setext because ATX is single-line and
            // would split a heading the parser legitimately produces.
            let setext_representable = matches!(node.depth, 1 | 2);
            Ok(match node.kind {
                HeadingKind::Setext if setext_representable => {
                    let marker = if node.depth == 1 { '=' } else { '-' };
                    let underline = marker.to_string().repeat(content.len().max(3));
                    let ends_with_text_pipe = matches!(
                        node.children.last(),
                        Some(Inline::Text(text)) if text.value.trim_end().ends_with('|')
                    );
                    let content = if node.depth == 2 && ends_with_text_pipe {
                        escape_pipe_ending_table_header(content)
                    } else {
                        content
                    };
                    let mut content = content;
                    keep_first_line_off_html_block(&node.children, &mut content);
                    let mut content = indent_block_starting_continuations(content);
                    if let Some(last_line_start) = content.rfind('\n').map(|end| end + 1) {
                        // A continuation line would read as a table header
                        // over the underline; indented, it reads as before.
                        if gfm_table_can_start_source(&content[last_line_start..], &underline) {
                            content.insert_str(last_line_start, "    ");
                        }
                    }
                    format!("{content}\n{underline}")
                }
                _ if content.is_empty() => "#".repeat(node.depth as usize),
                _ => format!(
                    "{} {}",
                    "#".repeat(node.depth as usize),
                    escape_atx_heading_content(&content)
                ),
            })
        }
        Block::ThematicBreak(node) => Ok(match node.marker {
            // A Dash break is normally written contiguous (`---`) — the form
            // that survives after a `-` bullet list, where the spaced `- - -`
            // would be re-read as nested list items. The one exception is the
            // document start, where a contiguous `---` opens frontmatter, so the
            // spaced form (which is not a frontmatter fence) is used there.
            ThematicBreakMarker::Dash if at_document_start => "- - -".into(),
            ThematicBreakMarker::Dash => "---".into(),
            ThematicBreakMarker::Asterisk => "***".into(),
            ThematicBreakMarker::Underscore => "___".into(),
        }),
        Block::BlockQuote(node) => {
            let inner = serialize_blocks_at_start(&node.children, options, false)?;
            if inner.is_empty() {
                Ok(">".into())
            } else {
                Ok(prefix_lines(&inner, "> "))
            }
        }
        Block::Alert(node) => serialize_alert(node, options),
        Block::List(node) => serialize_list(node, options),
        Block::DescriptionList(node) => serialize_description_list(node, options),
        Block::CodeBlock(node) => serialize_code_block(node, options),
        // An HTML block's lines are joined with `\n`, so a value ending in one
        // ends with an empty line that belongs to the block (an unclosed
        // comment, say); it is written as it is.
        Block::HtmlBlock(node) => Ok(node.value.clone()),
        Block::HtmlContainer(node) => serialize_html_container(node, options),
        Block::Definition(node) => {
            let destination = serialize_destination_kind(
                &node.destination,
                node.destination_kind,
                InlineSerializeContext::default(),
            );
            let mut label = if node.meta.span.is_some() {
                escape_definition_label_source(&node.label)
            } else {
                escape_reference_label_with_pipe(&node.label, false)
            };
            if node.meta.span.is_none() && label.starts_with('^') {
                label.insert(0, '\\');
            }
            let mut output = format!("[{}]: {}", label, destination);
            if let (Some(title), Some(title_kind)) = (&node.title, node.title_kind) {
                output.push(' ');
                output.push_str(&serialize_title_kind(
                    title,
                    title_kind,
                    InlineSerializeContext::default(),
                ));
            }
            Ok(output)
        }
        Block::FootnoteDefinition(node) => {
            let inner = serialize_blocks_at_start(&node.children, options, false)?;
            let label = if node.meta.span.is_some() {
                escape_footnote_label_source(&node.label)
            } else {
                escape_footnote_label_semantic(&node.label)
            };
            Ok(format!("[^{}]: {}", label, indent_continuation(&inner)))
        }
        Block::Table(node) => serialize_table(node, options),
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
        Block::MdxEsm(node) => Ok(node.value.clone()),
        Block::MdxExpression(node) => Ok(format!("{{{}}}", node.value)),
        Block::MdxJsx(node) => Ok(node.value.clone()),
        Block::LeafDirective(node) => Ok(format!(
            "::{}{}{}",
            node.name,
            serialize_directive_label(&node.label, options)?,
            serialize_attributes(&node.attributes)
        )),
        Block::ContainerDirective(node) => {
            let inner = serialize_blocks_at_start(&node.children, options, false)?;
            let fence = directive_fence(&inner);
            Ok(format!(
                "{fence}{}{}{}\n{}\n{fence}",
                node.name,
                serialize_directive_label(&node.label, options)?,
                serialize_attributes(&node.attributes),
                inner
            ))
        }
    }
}

fn serialize_html_container(
    node: &HtmlContainer,
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
    match &node.content {
        HtmlContainerContent::Blocks(children) => {
            let inner = serialize_blocks_at_start(children, options, false)?;
            if inner.is_empty() {
                Ok(format!("{}\n{}", node.opening.raw, node.closing.raw))
            } else {
                Ok(format!(
                    "{}\n{}\n\n{}",
                    node.opening.raw, inner, node.closing.raw
                ))
            }
        }
        HtmlContainerContent::Inlines(children) => Ok(format!(
            "{}{}{}",
            node.opening.raw,
            serialize_inlines(children, options)?,
            node.closing.raw
        )),
    }
}

/// Escape a trailing `#`-run in ATX heading content so it is not consumed as a
/// closing hash sequence. CommonMark treats a final run of `#` preceded by
/// whitespace (after trailing whitespace is trimmed) as the optional closing
/// sequence; escaping the first `#` of that run keeps it as literal text.
/// `content`, ending in text, with an unescaped `|` that ends its last line
/// escaped: above a `---` underline, a line ending in a bare pipe is a one-cell
/// table header and the underline its delimiter row.
fn escape_pipe_ending_table_header(content: String) -> String {
    let last_line = content.rsplit('\n').next().unwrap_or_default();
    let trimmed = last_line.trim_end();
    let Some(before_pipe) = trimmed.strip_suffix('|') else {
        return content;
    };
    let backslashes = before_pipe
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\\')
        .count();
    if backslashes % 2 == 1 {
        return content;
    }
    let pipe = content.len() - (last_line.len() - trimmed.len()) - 1;
    let mut escaped = content;
    escaped.insert(pipe, '\\');
    escaped
}

fn escape_atx_heading_content(content: &str) -> String {
    let trimmed_len = content.trim_end_matches([' ', '\t']).len();
    let trimmed = &content[..trimmed_len];
    let hash_start = trimmed.trim_end_matches('#').len();
    let preceded_by_whitespace = trimmed[..hash_start]
        .chars()
        .next_back()
        .is_some_and(|char| char == ' ' || char == '\t');
    if hash_start == trimmed_len || !preceded_by_whitespace {
        return content.into();
    }
    let mut output = String::with_capacity(content.len() + 1);
    output.push_str(&content[..hash_start]);
    output.push('\\');
    output.push_str(&content[hash_start..]);
    output
}

fn serialize_paragraph(
    node: &Paragraph,
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
    let render = |run_style: RunStyle, raw_edge: Option<char>| -> Result<String, SerializeError> {
        let context = InlineSerializeContext {
            run_style,
            raw_edge,
            ..InlineSerializeContext::block_content()
        };
        let mut output = serialize_inlines_with_context(&node.children, options, context)?;
        keep_first_line_off_html_block(&node.children, &mut output);
        Ok(indent_block_starting_continuations(output))
    };
    let output = render(RunStyle::Plain, None)?;
    // A strong or emphasis run abutting another splits on reparse only as its
    // flanking allows, which the rest of the paragraph decides; one beside a
    // `~` opens or closes only as the GFM bonus for a raw `~` allows, and one
    // beside a text `*` may take that `*` into its run. When the plain
    // rendering does not read back, the first other style that does is taken.
    let mut runs = RunNeighbours::default();
    runs.read(&node.children, 0);
    let RunNeighbours {
        abut_runs,
        edge_tildes,
        edge_stars,
    } = runs;
    if (abut_runs || edge_tildes || edge_stars) && !reparses_to(&output, &node.children) {
        let styles = [
            RunStyle::Plain,
            RunStyle::StrongUnderscore,
            RunStyle::InnerUnderscore,
            RunStyle::AllStar,
            RunStyle::OuterUnderscore,
        ];
        let raw_edges = [None, edge_tildes.then_some('~'), edge_stars.then_some('*')];
        let alternates = raw_edges
            .into_iter()
            .enumerate()
            .filter(|&(at, raw_edge)| at == 0 || raw_edge.is_some())
            .flat_map(|(_, raw_edge)| styles.map(|style| (style, raw_edge)))
            .skip(1);
        for (style, raw_edge) in alternates {
            let alternate = render(style, raw_edge)?;
            if reparses_to(&alternate, &node.children) {
                return Ok(alternate);
            }
        }
    }
    Ok(output)
}

/// What sits beside the strong and emphasis runs of a paragraph, at any
/// depth within runs.
#[derive(Default)]
struct RunNeighbours {
    /// Two runs sit side by side, a run opens or closes right beside one
    /// inside it, or a strong holds a strong or an emphasis an emphasis.
    abut_runs: bool,
    /// A text opens or closes with a `~` right beside a run's delimiter.
    edge_tildes: bool,
    /// The same with a `*`.
    edge_stars: bool,
}

impl RunNeighbours {
    /// Reads `inlines`; `inside` holds a bit for each run kind around them,
    /// `1` for strong and `2` for emphasis.
    fn read(&mut self, inlines: &[Inline], inside: u8) {
        for (index, inline) in inlines.iter().enumerate() {
            let (children, kind) = match inline {
                Inline::Text(node) => {
                    let after_run = (index == 0 && inside != 0)
                        || index
                            .checked_sub(1)
                            .is_some_and(|previous| is_attention_run(&inlines[previous]));
                    let before_run = (index + 1 == inlines.len() && inside != 0)
                        || inlines.get(index + 1).is_some_and(is_attention_run);
                    let value = node.value.as_bytes();
                    for (edge, found) in
                        [(b'~', &mut self.edge_tildes), (b'*', &mut self.edge_stars)]
                    {
                        *found |= (after_run && value.first() == Some(&edge))
                            || (before_run && value.last() == Some(&edge));
                    }
                    continue;
                }
                Inline::Strong(node) => (&node.children, 1),
                Inline::Emphasis(node) => (&node.children, 2),
                _ => continue,
            };
            self.abut_runs |= inside & kind != 0
                || inlines.get(index + 1).is_some_and(is_attention_run)
                || children.first().is_some_and(is_attention_run)
                || children.last().is_some_and(is_attention_run);
            self.read(children, inside | kind);
        }
    }
}

fn is_attention_run(inline: &Inline) -> bool {
    matches!(inline, Inline::Strong(_) | Inline::Emphasis(_))
}

/// `rendered` text with the run of `edge` chars at its start, or at its end,
/// written raw where it was escaped or written as a character reference.
fn unescape_edge(rendered: &str, edge: char, at_start: bool, at_end: bool) -> String {
    let escaped = alloc::format!("\\{edge}");
    let reference = alloc::format!("&#x{:X};", edge as u32);
    let mut text = rendered;
    let mut head = 0;
    if at_start {
        while let Some(rest) = text
            .strip_prefix(escaped.as_str())
            .or_else(|| text.strip_prefix(reference.as_str()))
        {
            head += 1;
            text = rest;
        }
    }
    let mut tail = 0;
    if at_end {
        loop {
            if let Some(before) = text.strip_suffix(reference.as_str()) {
                text = before;
            } else if let Some(before) = text.strip_suffix(escaped.as_str()) {
                let backslashes = before.len() - before.trim_end_matches('\\').len();
                if backslashes % 2 == 1 {
                    break;
                }
                text = before;
            } else {
                break;
            }
            tail += 1;
        }
    }
    let mut output = String::with_capacity(rendered.len());
    output.extend(core::iter::repeat_n(edge, head));
    output.push_str(text);
    output.extend(core::iter::repeat_n(edge, tail));
    output
}

/// Whether `markdown` parses, under the default dialect, to one paragraph
/// holding `inlines` (spans aside).
fn reparses_to(markdown: &str, inlines: &[Inline]) -> bool {
    let document = crate::options::SyntaxOptions::default()
        .parse(markdown)
        .document;
    match document.children.as_slice() {
        [Block::Paragraph(paragraph)] => {
            debug_without_spans(&paragraph.children) == debug_without_spans(inlines)
        }
        _ => false,
    }
}

/// The debug form of `inlines` with every span written as `None`.
fn debug_without_spans(inlines: &[Inline]) -> String {
    let debug = format!("{inlines:?}");
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug.as_str();
    while let Some(start) = rest.find("Some(Span { ") {
        out.push_str(&rest[..start]);
        out.push_str("None");
        let end = rest[start..]
            .find("})")
            .map_or(rest.len(), |end| start + end + 2);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Keeps the first line of inline content that would start an HTML block
/// from starting one. Text gets an escape. Raw HTML takes none; only a
/// complete tag alone on its line (type 7) reaches here outside a definition's
/// paragraph. Before a line ending, as `<a>&#x20;\n…` parses, a trailing space
/// written as a reference keeps the line the paragraph's; content that is
/// that line alone, or a tag that interrupts, follows a definition, which
/// `serialize_blocks_at_start` writes it after.
fn keep_first_line_off_html_block(children: &[Inline], output: &mut String) {
    let Some(offset) = paragraph_html_block_escape_offset(output) else {
        return;
    };
    if matches!(children.first(), Some(Inline::Html(_))) {
        let first_line = output.split('\n').next().unwrap_or("");
        if !line_starts_interrupting_html_block(first_line) {
            if let Some(line_end) = output.find('\n') {
                output.insert_str(line_end, "&#x20;");
            }
        }
    } else {
        output.insert(offset, '\\');
    }
}

/// Indents each continuation line of inline content that would start a block
/// (a line inside a code span, raw HTML, or a link title, which text escaping
/// does not reach) past a block start; the paragraph drops that indentation.
fn indent_block_starting_continuations(output: String) -> String {
    let mut lines = output.split('\n');
    let Some(first) = lines.next() else {
        return output;
    };
    let mut previous = first;
    let mut indented = None::<String>;
    let mut written = first.len();
    for line in lines {
        if continuation_line_breaks_paragraph(previous, line) {
            let result = indented.get_or_insert_with(|| String::from(&output[..written]));
            result.push_str("\n    ");
            result.push_str(line);
        } else if let Some(result) = indented.as_mut() {
            result.push('\n');
            result.push_str(line);
        }
        written += 1 + line.len();
        previous = line;
    }
    indented.unwrap_or(output)
}

fn paragraph_html_block_escape_offset(input: &str) -> Option<usize> {
    let first_line = input.split('\n').next().unwrap_or(input);
    if !line_starts_html_block(first_line) {
        return None;
    }

    Some(
        first_line
            .as_bytes()
            .iter()
            .take_while(|byte| **byte == b' ')
            .count(),
    )
}

fn serialize_alert(node: &Alert, options: &SerializeOptions) -> Result<String, SerializeError> {
    let mut output = String::from("> [!");
    output.push_str(alert_kind_name(node.kind));
    output.push(']');
    if let Some(title) = &node.title {
        if !title.is_empty() {
            output.push(' ');
            output.push_str(&escape_alert_title(title));
        }
    }
    let inner = serialize_blocks_at_start(&node.children, options, false)?;
    if !inner.is_empty() {
        output.push('\n');
        output.push_str(&prefix_lines(&inner, "> "));
    }
    Ok(output)
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

fn escape_alert_title(input: &str) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            '\n' | '\r' => output.push(' '),
            char if char.is_control() => output.push_str(&format!("&#x{:X};", char as u32)),
            _ => output.push(char),
        }
    }
    output
}

fn serialize_list(node: &List, options: &SerializeOptions) -> Result<String, SerializeError> {
    serialize_list_with_marker_spacing(node, options, "", " ")
}

fn serialize_list_with_marker_spacing(
    node: &List,
    options: &SerializeOptions,
    marker_prefix: &str,
    marker_padding: &str,
) -> Result<String, SerializeError> {
    let mut output = String::new();
    for (index, item) in node.children.iter().enumerate() {
        if index > 0 {
            if node.tight {
                output.push('\n');
            } else {
                output.push_str("\n\n");
            }
        }
        let list_delimiter = if node.ordered {
            if options.ordered_delimiter == SerializeOptions::default().ordered_delimiter {
                node.delimiter
            } else {
                options.ordered_delimiter
            }
        } else if options.bullet == SerializeOptions::default().bullet {
            node.delimiter
        } else {
            options.bullet
        };
        let marker = if node.ordered {
            let start = node.start.unwrap_or(1).saturating_add(index as u64);
            let delimiter = ordered_list_marker(list_delimiter);
            format!("{marker_prefix}{start}{delimiter}{marker_padding}")
        } else {
            format!(
                "{marker_prefix}{}{marker_padding}",
                unordered_list_marker(list_delimiter)
            )
        };
        let mut inner = serialize_item_blocks(&item.children, options, node.tight)?;
        if let Some(checked) = item.checked {
            if let Some(rest) = inner.strip_prefix("- ") {
                inner = rest.into();
            }
            let checkbox = if checked { "[x] " } else { "[ ] " };
            inner = format!("{checkbox}{inner}");
        }
        if !node.tight
            && node.children.len() == 1
            && matches!(item.children.as_slice(), [Block::Paragraph(_)])
            && !inner.is_empty()
        {
            output.push_str(marker.trim_end());
            output.push_str("\n\n");
            output.push_str(&prefix_lines(&inner, &" ".repeat(marker.len())));
            continue;
        }
        let first_line = inner.split('\n').next().unwrap_or("");
        let may_break = first_line.starts_with(['-', '*', '_']);
        if inner.starts_with([' ', '\t'])
            || (may_break && is_thematic_break_line(&format!("{marker}{first_line}")))
        {
            // Content after the marker's padding would move the item's content
            // column, so whitespace that opens the item's first block (an HTML
            // block's indentation) starts on the line after the marker; so does
            // a first line that would make the marker's line a thematic break.
            // An item that starts blank has its content one column past the
            // marker, whatever padding the other items use.
            let content_indent = marker.trim_end().len() + 1;
            output.push_str(marker.trim_end());
            output.push('\n');
            output.push_str(&prefix_lines(&inner, &" ".repeat(content_indent)));
            continue;
        }
        output.push_str(&marker);
        output.push_str(&indent_after_first_line(&inner, marker.len()));
    }
    Ok(output)
}

/// Whether `line` is a thematic break: up to three spaces, then three or more
/// of one of `-`, `*`, `_`, with only spaces and tabs between and after them.
fn is_thematic_break_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return false;
    }
    let Some(marker) = trimmed
        .chars()
        .next()
        .filter(|char| matches!(char, '-' | '*' | '_'))
    else {
        return false;
    };
    trimmed
        .chars()
        .all(|char| matches!(char, ' ' | '\t') || char == marker)
        && trimmed.chars().filter(|char| *char == marker).count() >= 3
}

fn serialize_item_blocks(
    blocks: &[Block],
    options: &SerializeOptions,
    tight: bool,
) -> Result<String, SerializeError> {
    let mut output = String::new();
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            if tight {
                output.push('\n');
            } else {
                output.push_str("\n\n");
            }
        }
        output.push_str(&serialize_block(block, options, false)?);
        if tight
            && matches!(block, Block::BlockQuote(_))
            && matches!(blocks.get(index + 1), Some(Block::Paragraph(_)))
        {
            // The paragraph's first line would continue the quote's paragraph
            // lazily; an empty quote line ends that paragraph first.
            output.push_str("\n>");
        }
    }
    Ok(output)
}

fn serialize_description_list(
    node: &DescriptionList,
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
    let mut output = String::new();
    for (item_index, item) in node.children.iter().enumerate() {
        if item_index > 0 {
            output.push_str(if node.tight { "\n" } else { "\n\n" });
        }
        output.push_str(&serialize_inlines(&item.term, options)?);
        for (detail_index, detail) in item.details.iter().enumerate() {
            if node.tight && detail.children.len() == 1 {
                if let Block::Paragraph(paragraph) = &detail.children[0] {
                    output.push('\n');
                    output.push_str(": ");
                    output.push_str(&serialize_inlines(&paragraph.children, options)?);
                    continue;
                }
            }
            // A loose list is re-parsed as loose only through an intra-item blank;
            // the parser treats blanks BETWEEN items as tight-preserving group
            // separators. Encode the looseness with a blank line before the term's
            // first definition marker (a `blank_after_term`), so the round trip
            // keeps `tight=false`.
            if !node.tight && detail_index == 0 {
                output.push('\n');
            }
            output.push_str("\n:");
            let inner = if node.tight {
                serialize_item_blocks(&detail.children, options, true)?
            } else {
                serialize_blocks_at_start(&detail.children, options, false)?
            };
            if !inner.is_empty() {
                output.push('\n');
                output.push_str(&indent_lines(&inner, 4));
            }
        }
    }
    Ok(output)
}

fn serialize_code_block(
    node: &CodeBlock,
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
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
            Ok(output)
        }
        CodeBlockKind::Fenced { marker, length } => {
            let marker = code_block_fence_marker(node, marker, options);
            let (fence, indent) = code_block_fence(&node.value, marker, length.max(3));
            let mut opener = fence.clone();
            if let Some(info) = &node.info {
                opener.push(' ');
                opener.push_str(&escape_code_info(info));
            }
            let body = fenced_body(&opener, &node.value, &fence);
            Ok(if indent == 0 {
                body
            } else {
                prefix_lines(&body, &" ".repeat(indent))
            })
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

fn code_block_fence_marker(
    node: &CodeBlock,
    marker: FenceMarker,
    options: &SerializeOptions,
) -> FenceMarker {
    if node.info.as_deref().is_some_and(|info| info.contains('`')) {
        return FenceMarker::Tilde;
    }
    if options.fence_marker == SerializeOptions::default().fence_marker {
        marker
    } else {
        options.fence_marker
    }
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
                output.push_str(&format!("&#x{:X};", char as u32));
            }
            '\n' => output.push_str("&#xA;"),
            '\r' => output.push_str("&#xD;"),
            '\t' => output.push(char),
            char if char.is_control() => output.push_str(&format!("&#x{:X};", char as u32)),
            '\\' | '&' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn serialize_table(node: &Table, options: &SerializeOptions) -> Result<String, SerializeError> {
    let header = &node.rows[0];
    let mut output = serialize_table_row(header, options)?;
    output.push('\n');
    output.push('|');
    output.push(' ');
    output.push_str(
        &node
            .alignments
            .iter()
            .map(|alignment| match alignment {
                TableAlignment::None => "---",
                TableAlignment::Left => ":---",
                TableAlignment::Center => ":---:",
                TableAlignment::Right => "---:",
            })
            .collect::<Vec<_>>()
            .join(" | "),
    );
    output.push(' ');
    output.push('|');
    for row in node.rows.iter().skip(1) {
        output.push('\n');
        output.push_str(&serialize_table_row(row, options)?);
    }
    Ok(output)
}

fn serialize_table_row(
    row: &TableRow,
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
    let mut cells = Vec::new();
    for cell in &row.cells {
        let cell = serialize_inlines_with_context(
            &cell.children,
            options,
            InlineSerializeContext::table_cell(),
        )?;
        cells.push(escape_cell_delimiter_pipes(cell));
    }
    Ok(format!("| {} |", cells.join(" | ")))
}

/// How strong and emphasis runs choose between `*` and `_`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RunStyle {
    /// Each run reads its neighbours: `_` where a `*` would join a run beside
    /// it, `*` otherwise.
    #[default]
    Plain,
    /// As `Plain`, and a strong whose content opens or closes with a `*` run
    /// is written `__`.
    StrongUnderscore,
    /// Every run is written with `*` where nothing before it would join it.
    AllStar,
    /// An emphasis with no strong or emphasis inside is written with `_`, and
    /// every other run with `*`.
    InnerUnderscore,
    /// A run inside no other is written with `_`, and every other with `*`.
    OuterUnderscore,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct InlineSerializeContext {
    table_cell: bool,
    avoid_star_edges: bool,
    /// Inside an `_`-delimited emphasis, where a `_` in text that can close
    /// would close it on reparse.
    in_underscore_emphasis: bool,
    /// The inlines open a line of the block, rather than following a
    /// delimiter such as a link's `[` on it.
    opens_line: bool,
    /// For a text, the delimiter chars that the inlines after it may write;
    /// for nested inlines, those after their parent, at every level.
    written_later: DelimiterChars,
    /// The same for the inlines before.
    written_before: DelimiterChars,
    /// For a text, whether it opens a line of the block.
    text_opens_line: bool,
    /// How strong and emphasis runs choose their char (see
    /// `serialize_paragraph`).
    run_style: RunStyle,
    /// The inlines open a span delimited by a run such as `*` or `++`, which
    /// a line ending right after it could not open.
    opens_span: bool,
    /// The char whose run opening or closing a text beside a strong or
    /// emphasis delimiter is written raw (see `serialize_paragraph`).
    raw_edge: Option<char>,
}

/// A set of the chars `*`, `_`, `~`, `+`, `=`, `^`, `|`, `$`, and `:`, which
/// delimit inline spans or shortcodes that pair across sibling inlines, and
/// `>`, which ends raw HTML or an autolink that a `<` before it may open.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DelimiterChars(u16);

impl DelimiterChars {
    const CHARS: [char; 10] = ['*', '_', '~', '+', '=', '^', '|', '$', ':', '>'];

    /// The set holding `char` when it is one of [`Self::CHARS`], in order.
    const fn of_char(char: char) -> Self {
        Self(match char {
            '*' => 1,
            '_' => 1 << 1,
            '~' => 1 << 2,
            '+' => 1 << 3,
            '=' => 1 << 4,
            '^' => 1 << 5,
            '|' => 1 << 6,
            '$' => 1 << 7,
            ':' => 1 << 8,
            '>' => 1 << 9,
            _ => 0,
        })
    }

    fn of_str(input: &str) -> Self {
        // Every char of the set is ASCII, so the bytes suffice.
        input.bytes().fold(Self(0), |set, byte| {
            set.union(Self::of_char(char::from(byte)))
        })
    }

    const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn contains(self, char: char) -> bool {
        let bit = Self::of_char(char).0;
        bit != 0 && self.0 & bit == bit
    }

    /// The `>` and `$` in raw text, which can close what a char before it
    /// opens.
    fn raw_closers(raw: &str) -> Self {
        let mut set = Self(0);
        for char in ['>', '$'] {
            if raw.contains(char) {
                set = set.union(Self::of_char(char));
            }
        }
        set
    }

    /// The delimiter chars that `inline` may write that can pair with a run
    /// before it: a link's or image's text pairs only within its brackets.
    fn written_by(inline: &Inline) -> Self {
        let within = |own: &str, children: &[Inline]| {
            children.iter().fold(Self::of_str(own), |set, child| {
                set.union(Self::written_by(child))
            })
        };
        match inline {
            Inline::Text(node) => Self::of_str(&node.value),
            Inline::Emphasis(node) => within("*_", &node.children),
            Inline::Strong(node) => within("*_", &node.children),
            Inline::Underline(node) => within("_", &node.children),
            Inline::Delete(node) => within("~", &node.children),
            Inline::Insert(node) => within("+", &node.children),
            Inline::Mark(node) => within("=", &node.children),
            Inline::Subscript(node) => within("~", &node.children),
            Inline::Superscript(node) => within("^", &node.children),
            Inline::Spoiler(node) => within("|", &node.children),
            // Their contents pair with nothing outside them; only a `>` in
            // them can end raw HTML that a `<` before them opens, and a `$`
            // close math that a `$` before them opens, as a math span's own
            // `$` fence can.
            Inline::Math(MathInline { value, .. }) => {
                Self::of_char('$').union(Self::raw_closers(value))
            }
            Inline::Html(HtmlInline { value: raw, .. }) | Inline::Code(CodeInline { raw, .. }) => {
                Self::raw_closers(raw)
            }
            // A footnote's `^` can close a superscript before it; its label
            // or content pairs with nothing outside it but a `<`.
            Inline::FootnoteReference(node) => {
                let caret = Self::of_char('^');
                if node.label.contains('>') {
                    caret.union(Self::of_char('>'))
                } else {
                    caret
                }
            }
            Inline::InlineFootnote(node) => {
                let caret = Self::of_char('^');
                if within("", &node.children).contains('>') {
                    caret.union(Self::of_char('>'))
                } else {
                    caret
                }
            }
            // A URL's chars are scanned after the delimiters before it pair.
            Inline::Autolink(node) => Self::of_str(&node.destination).union(Self::of_char('>')),
            // A wiki link's text can close what a char before it opens.
            Inline::WikiLink(node) => Self::of_str(&node.target).union(Self::of_str(&node.label)),
            Inline::Shortcode(_) | Inline::TextDirective(_) => Self::of_char(':'),
            _ => Self(0),
        }
    }
}

impl InlineSerializeContext {
    const fn table_cell() -> Self {
        Self {
            table_cell: true,
            avoid_star_edges: false,
            in_underscore_emphasis: false,
            opens_line: false,
            written_later: DelimiterChars(0),
            written_before: DelimiterChars(0),
            text_opens_line: false,
            run_style: RunStyle::Plain,
            opens_span: false,
            raw_edge: None,
        }
    }

    /// The context of a span's content, delimited by `delimiter` on both
    /// sides.
    fn delimited_by(self, delimiter: char) -> Self {
        let delimiter = DelimiterChars::of_char(delimiter);
        Self {
            written_later: self.written_later.union(delimiter),
            written_before: self.written_before.union(delimiter),
            ..self
        }
    }

    const fn opening_span(self) -> Self {
        Self {
            opens_span: true,
            ..self
        }
    }

    const fn block_content() -> Self {
        Self {
            table_cell: false,
            avoid_star_edges: false,
            in_underscore_emphasis: false,
            opens_line: true,
            written_later: DelimiterChars(0),
            written_before: DelimiterChars(0),
            text_opens_line: false,
            run_style: RunStyle::Plain,
            opens_span: false,
            raw_edge: None,
        }
    }

    /// Whether the inlines sit inside a strong or emphasis.
    const fn inside_run(self) -> bool {
        self.avoid_star_edges || self.in_underscore_emphasis
    }

    const fn avoiding_star_edges(self) -> Self {
        Self {
            avoid_star_edges: true,
            ..self
        }
    }

    const fn inside_underscore_emphasis(self) -> Self {
        Self {
            in_underscore_emphasis: true,
            ..self
        }
    }
}

fn serialize_inlines(
    inlines: &[Inline],
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
    serialize_inlines_with_context(inlines, options, InlineSerializeContext::block_content())
}

/// Escape a trailing unescaped `!` already in `output` before emitting a
/// following `[`-starting node (link / reference / footnote / wikilink), so the pair does
/// not reparse as an image (`![…]`). The within-text `!`-before-`[` escaper
/// only sees a single text node, so this handles the cross-node boundary.
fn escape_trailing_bang(output: &mut String) {
    if output.ends_with('!') && !output.ends_with("\\!") {
        output.pop();
        output.push_str("\\!");
    }
}

// A GFM literal autolink is serialized as its raw URL text. If the preceding
// output ends with `<`, that `<` plus the URL plus a following `>` could be
// reparsed as an angle autolink (`<http://x>`) instead of the literal. Escaping
// the trailing `<` keeps it literal text, so the URL stays a GFM literal on the
// round trip (`\<` before `http://…` is just text + literal).
fn escape_trailing_less_than(output: &mut String) {
    if output.ends_with('<') && !output.ends_with("\\<") {
        output.pop();
        output.push_str("\\<");
    }
}

// A GFM bare-email literal anchors at its leftmost run of email-local chars
// (`[A-Za-z0-9.+_-]`). If the preceding output ends with such a char, the
// reparse would extend the email's local part leftward into that text (e.g.
// `A` + `i@i.a` → `Ai@i.a`). Re-emit the trailing email-local char as a numeric
// character reference (which decodes back to the same text but is not an
// email-local char), preserving the boundary on the round trip.
fn escape_trailing_email_local(output: &mut String) {
    let Some(last) = output.chars().next_back() else {
        return;
    };
    // Only an ASCII alphanumeric immediately before the email forces a leftward
    // re-anchor on reparse (the local part starts at the leftmost local-char
    // run, and the dispatch reaches that alnum first). The punctuation
    // local-chars (`.+-_`) are handled by the text serializer's own escaping.
    if !last.is_ascii_alphanumeric() {
        return;
    }
    output.pop();
    output.push_str(&alloc::format!("&#{};", last as u32));
}

// True when `inline` is a GFM literal autolink (its raw URL serialization can
// re-absorb a following text char on reparse).
fn is_gfm_literal_autolink(inline: &Inline) -> bool {
    matches!(
        inline,
        Inline::Autolink(node) if matches!(node.kind, AutolinkKind::GfmLiteral { .. })
    )
}

// True when `inline` is a shortcut link or image reference or a footnote
// reference, whose `[label]` a following `(` would turn into an inline link,
// and a following `:` at the start of a line into a definition.
fn is_shortcut_reference(inline: &Inline) -> bool {
    matches!(
        inline,
        Inline::LinkReference(LinkReference {
            kind: ReferenceKind::Shortcut,
            ..
        }) | Inline::ImageReference(ImageReference {
            kind: ReferenceKind::Shortcut,
            ..
        }) | Inline::FootnoteReference(_)
    )
}

// The escaped first character of text after a shortcut reference, when that
// character would re-read the reference's brackets: a `(` always, and a `:`
// when the reference opens the inline sequence, where a line can start.
fn escape_leading_char_after_shortcut(value: &str, reference_index: usize) -> Option<&str> {
    match value.as_bytes().first() {
        Some(b'(') => Some("\\("),
        Some(b':') if reference_index == 0 => Some("\\:"),
        _ => None,
    }
}

fn is_gfm_literal_email(inline: &Inline) -> bool {
    matches!(
        inline,
        Inline::Autolink(node)
            if matches!(&node.kind, AutolinkKind::GfmLiteral { original }
                if node.destination.strip_prefix("mailto:") == Some(original.as_str()))
    )
}

/// The source spelling of a GFM literal autolink.
fn literal_autolink_original(inline: &Inline) -> Option<&str> {
    match inline {
        Inline::Autolink(Autolink {
            kind: AutolinkKind::GfmLiteral { original },
            ..
        }) => Some(original),
        _ => None,
    }
}

/// The spelling of the literal autolink that `inline` is, or that its last
/// child is, through spans.
fn last_literal_autolink(inline: &Inline) -> Option<&str> {
    if let Some(original) = literal_autolink_original(inline) {
        return Some(original);
    }
    let children = match inline {
        Inline::Emphasis(node) => &node.children,
        Inline::Strong(node) => &node.children,
        Inline::Underline(node) => &node.children,
        Inline::Delete(node) => &node.children,
        Inline::Insert(node) => &node.children,
        Inline::Mark(node) => &node.children,
        Inline::Subscript(node) => &node.children,
        Inline::Superscript(node) => &node.children,
        Inline::Spoiler(node) => &node.children,
        _ => return None,
    };
    last_literal_autolink(children.last()?)
}

/// How `inline` is written when that takes no escaping: a literal autolink or
/// a shortcode, which a literal autolink's URL scan may run on into; empty
/// for any other inline.
fn plain_spelling(inline: &Inline) -> String {
    match inline {
        Inline::Shortcode(node) => alloc::format!(":{}:", node.name),
        _ => literal_autolink_original(inline).map_or(String::new(), String::from),
    }
}

/// Whether `rendered` text written right after the literal autolink
/// `original`, and before `following` (the [`plain_spelling`] of the next
/// inline), leaves the autolink as it is under at least one
/// autolink dialect (GFM, or GFM plus relaxed): the parser's own scan reads
/// exactly `original`. The dialect the document came from is not known here;
/// the source spelling keeps it under that one.
fn text_keeps_literal_autolink(original: &str, rendered: &str, following: &str) -> bool {
    let mut joined = String::from(original);
    joined.push_str(rendered);
    joined.push_str(following);
    literal_autolink_extents(&joined).contains(&Some(original.len()))
}

/// Spellings of `value`'s first char that a literal autolink's URL scan may
/// stop at, most readable first: a backslash escape for ASCII punctuation, then
/// a character reference, which the scan trims back off the URL's end.
fn leading_char_encodings(value: &str) -> Vec<(String, &str)> {
    let Some(first) = value.chars().next() else {
        return Vec::new();
    };
    let rest = &value[first.len_utf8()..];
    let mut encodings = Vec::new();
    if first.is_ascii_punctuation() {
        encodings.push((alloc::format!("\\{first}"), rest));
    }
    encodings.push((alloc::format!("&#x{:X};", first as u32), rest));
    encodings
}

fn serialize_inlines_with_context(
    inlines: &[Inline],
    options: &SerializeOptions,
    context: InlineSerializeContext,
) -> Result<String, SerializeError> {
    let opens_line = context.opens_line;
    let opens_span = context.opens_span;
    // Nested inlines follow their parent's opening delimiter.
    let base_context = InlineSerializeContext {
        opens_line: false,
        text_opens_line: false,
        opens_span: false,
        ..context
    };
    let mut output = String::new();
    let mut output_line = OutputLine::default();
    // The delimiter chars the inlines from each index on may write, and those
    // before each index, within this run and around it. A run of plain text
    // and breaks needs no more than the chars around it, so its inlines are
    // read only when a text holds such a char or an inline holds others.
    let needs_written = inlines.len() > 1
        && inlines.iter().any(|inline| match inline {
            Inline::Text(node) => node.value.contains(DelimiterChars::CHARS),
            Inline::SoftBreak(_) | Inline::LineBreak(_) => false,
            _ => true,
        });
    // Each inline's own chars, and the chars from each index on.
    let written = needs_written.then(|| {
        let own = inlines
            .iter()
            .map(DelimiterChars::written_by)
            .collect::<Vec<_>>();
        let mut from = vec![context.written_later; inlines.len() + 1];
        for index in (0..inlines.len()).rev() {
            from[index] = from[index + 1].union(own[index]);
        }
        (own, from)
    });
    let mut written_until = context.written_before;
    // Whether an earlier reference wrote a backtick in its raw label, which an
    // escaped backtick after it could close as a code span.
    let mut raw_backtick_before = false;
    // Where the output of the inline before the current one starts.
    let mut segment_start = 0;
    for (index, inline) in inlines.iter().enumerate() {
        let written_before = written_until;
        let previous_start = segment_start;
        segment_start = output.len();
        let written_later = match &written {
            Some((own, from)) => {
                written_until = written_until.union(own[index]);
                from[index + 1]
            }
            None => context.written_later,
        };
        let context = InlineSerializeContext {
            written_later,
            written_before,
            ..base_context
        };
        match inline {
            Inline::Text(node) => {
                // A literal autolink just before the text, ending this run's
                // previous inline or the last span inside it: its spelling,
                // and the span delimiters written after it.
                let autolink_before = index.checked_sub(1).and_then(|prev| {
                    let original = last_literal_autolink(&inlines[prev])?;
                    // The URL may itself end with a delimiter char, so each
                    // split of the trailing delimiters is tried, shortest tail
                    // first.
                    let segment = &output[previous_start..];
                    let delimiters = segment.len()
                        - segment
                            .trim_end_matches(['*', '_', '~', '=', '+', '^', '|'])
                            .len();
                    (0..=delimiters).find_map(|tail| {
                        let body = &segment[..segment.len() - tail];
                        body.ends_with(original)
                            .then(|| (original, &segment[segment.len() - tail..]))
                    })
                });
                let before_literal_autolink =
                    inlines.get(index + 1).is_some_and(is_gfm_literal_autolink);
                let at_line_start = output_line.len(&output) == 0;
                let opens_block_line = breaks_line_start(&output, &mut output_line, opens_line);
                let at_line_end = text_is_at_line_end(inlines, index);
                let text_context = InlineSerializeContext {
                    text_opens_line: opens_block_line,
                    ..context
                };

                // Trailing guard: when this text is immediately followed by a
                // www/http/email literal, its trailing whitespace must survive
                // as a real whitespace preceder. A trailing space/tab is
                // otherwise re-encoded (`&#x20;`/`&#x9;`) at an edge or as a
                // control char, which would break the literal's left boundary on
                // reparse — emit the trailing space/tab run literally instead.
                let render = |lead: &str, body: &str| {
                    let head = body.trim_end_matches([' ', '\t']);
                    // Whitespace that opens a line stays encoded: written
                    // literally, the line would drop it.
                    let whole_line_start = opens_block_line && lead.is_empty() && head.is_empty();
                    let (escape_body, trailing_ws) = if before_literal_autolink && !whole_line_start
                    {
                        (head, &body[head.len()..])
                    } else {
                        (body, "")
                    };
                    let mut rendered = String::with_capacity(lead.len() + body.len() + 8);
                    rendered.push_str(lead);
                    rendered.push_str(&escape_text_with_context(
                        escape_body,
                        lead.is_empty() && trailing_ws.len() != body.len() && at_line_start,
                        trailing_ws.is_empty() && at_line_end,
                        text_context,
                    ));
                    rendered.push_str(trailing_ws);
                    rendered
                };

                let after_shortcut = index
                    .checked_sub(1)
                    .filter(|&prev| is_shortcut_reference(&inlines[prev]));
                let mut rendered = match after_shortcut.and_then(|reference| {
                    escape_leading_char_after_shortcut(&node.value, reference)
                }) {
                    Some(escaped) => render(escaped, &node.value[1..]),
                    None => render("", &node.value),
                };
                if let Some(edge) = context.raw_edge {
                    let at_start = (index == 0 && opens_span)
                        || index
                            .checked_sub(1)
                            .is_some_and(|previous| is_attention_run(&inlines[previous]));
                    let at_end = (index + 1 == inlines.len() && opens_span)
                        || inlines.get(index + 1).is_some_and(is_attention_run);
                    if at_start || at_end {
                        rendered = unescape_edge(&rendered, edge, at_start, at_end);
                    }
                }
                // Leading guard: text right after a literal autolink must not
                // extend its URL on reparse. When it would, its first char is
                // written in the first form the URL scan stops at.
                if let Some((original, tail)) = autolink_before {
                    let following = inlines.get(index + 1).map_or(String::new(), plain_spelling);
                    let keeps = |rendered: &str| {
                        text_keeps_literal_autolink(
                            original,
                            &format!("{tail}{rendered}"),
                            &following,
                        )
                    };
                    if !keeps(&rendered) {
                        for (lead, rest) in leading_char_encodings(&node.value) {
                            let candidate = render(&lead, rest);
                            if keeps(&candidate) {
                                rendered = candidate;
                                break;
                            }
                        }
                    }
                }
                // A scheme char ending the text would join the scheme of a
                // literal autolink after it (`://x` or `p://x` under the
                // relaxed dialect).
                if inlines
                    .get(index + 1)
                    .and_then(literal_autolink_original)
                    .is_some_and(|original| {
                        let scheme = original
                            .bytes()
                            .take_while(|byte| {
                                byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-')
                            })
                            .count();
                        original[scheme..].starts_with("://")
                    })
                {
                    if let Some(last) = rendered.chars().next_back().filter(|char| {
                        char.is_ascii_alphanumeric() || matches!(char, '+' | '.' | '-')
                    }) {
                        if !ends_with_unescaped(&rendered, last) || last.is_ascii_alphanumeric() {
                            if last.is_ascii_alphanumeric() {
                                rendered.pop();
                                rendered.push_str(&alloc::format!("&#x{:X};", last as u32));
                            }
                        } else {
                            rendered.insert(rendered.len() - 1, '\\');
                        }
                    }
                }
                // An escaped backtick still closes a code span that a raw
                // backtick before it opens; a character reference does not.
                if raw_backtick_before && rendered.contains("\\`") {
                    rendered = rendered.replace("\\`", "&#96;");
                }
                // A `:` opening the text would close a shortcode that a bare
                // text directive before it opens.
                if index.checked_sub(1).is_some_and(|prev| {
                    matches!(&inlines[prev], Inline::TextDirective(directive)
                        if directive.label.is_empty() && directive.attributes.is_empty())
                }) && rendered.starts_with(':')
                {
                    rendered.insert(0, '\\');
                }
                // A `:` ending the text would open a shortcode that a `:` in
                // the literal autolink after it closes, or that a span after it
                // names with its `++` or `_` delimiters.
                if (inlines
                    .get(index + 1)
                    .and_then(literal_autolink_original)
                    .is_some()
                    || (matches!(
                        inlines.get(index + 1),
                        Some(
                            Inline::Insert(_)
                                | Inline::Underline(_)
                                | Inline::Strong(_)
                                | Inline::Emphasis(_)
                        )
                    ) && written_later.contains(':')))
                    && ends_with_unescaped(&rendered, ':')
                {
                    rendered.insert(rendered.len() - 1, '\\');
                }
                output.push_str(&rendered);
            }
            Inline::Escape(node) => {
                output.push('\\');
                output.push(node.value);
            }
            Inline::CharacterReference(node) => output.push_str(&node.reference),
            Inline::Emphasis(node) => {
                // Rendered as `_` content first: the choice below reads only
                // the children's edges and `*`s, which escaping a `_` that can
                // close does not change, so only the `*` choice renders them
                // again and nesting never multiplies the work.
                let children = serialize_inlines_with_context(
                    &node.children,
                    options,
                    context.inside_underscore_emphasis().opening_span(),
                )?;
                let touches_underscore = children.starts_with('_')
                    || children.ends_with('_')
                    || children.starts_with("\\_")
                    || children.ends_with("\\_");
                // An emphasis abutting a `*` already in the output (e.g. a
                // preceding `*`-emphasis) would otherwise merge into one run, so
                // switch this run to `_` when that does not introduce a new
                // `_`-collision with the children.
                // A raw edge `*` is meant to join the run.
                let abuts_star = ends_with_unescaped(&output, '*')
                    && !touches_underscore
                    && context.raw_edge != Some('*');
                // `_` neither opens after nor closes before an alphanumeric.
                let underscore_flanks = !output
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric)
                    && !matches!(
                        inlines.get(index + 1),
                        Some(Inline::Text(next)) if next.value.chars().next().is_some_and(char::is_alphanumeric)
                    );
                let innermost = !node
                    .children
                    .iter()
                    .any(|child| matches!(child, Inline::Strong(_) | Inline::Emphasis(_)));
                let prefer_underscore = underscore_flanks
                    && match context.run_style {
                        RunStyle::Plain | RunStyle::StrongUnderscore => {
                            (context.avoid_star_edges && !touches_underscore)
                                || abuts_star
                                || children.starts_with('*')
                                || children.ends_with('*')
                        }
                        RunStyle::AllStar => abuts_star,
                        RunStyle::InnerUnderscore => {
                            abuts_star || (innermost && !touches_underscore)
                        }
                        RunStyle::OuterUnderscore => {
                            abuts_star || (!context.inside_run() && !touches_underscore)
                        }
                    };
                let delimiter = if prefer_underscore { '_' } else { '*' };
                let children = if delimiter == '*' {
                    serialize_inlines_with_context(
                        &node.children,
                        options,
                        context.avoiding_star_edges().opening_span(),
                    )?
                } else {
                    children
                };
                output.push(delimiter);
                output.push_str(&children);
                output.push(delimiter);
            }
            Inline::Strong(node) => {
                let children = serialize_inlines_with_context(
                    &node.children,
                    options,
                    context.avoiding_star_edges().opening_span(),
                )?;
                // A `**` right after a `*` that closes a span would join its
                // run, so the strong is written with `__` there when `_` can
                // flank and its content does not touch `_`. Abutting nested
                // or following runs stay `**`: they split as written, and
                // `__` reparses as `Underline` when that construct is enabled,
                // which the serializer has no signal for.
                let after_star = (ends_with_unescaped(&output, '*')
                    && context.raw_edge != Some('*'))
                    || (context.run_style == RunStyle::StrongUnderscore
                        && (children.starts_with('*') || children.ends_with('*')));
                let underscore_fits = !children.starts_with('_')
                    && !ends_with_unescaped(&children, '_')
                    && !matches!(
                        inlines.get(index + 1),
                        Some(Inline::Text(next))
                            if next.value.chars().next().is_some_and(|char| char.is_alphanumeric() || char == '_')
                    );
                let outer_underscore = context.run_style == RunStyle::OuterUnderscore
                    && !context.inside_run()
                    && !output
                        .chars()
                        .next_back()
                        .is_some_and(char::is_alphanumeric);
                let delimiter = if (after_star || outer_underscore) && underscore_fits {
                    "__"
                } else {
                    "**"
                };
                output.push_str(delimiter);
                output.push_str(&children);
                output.push_str(delimiter);
            }
            Inline::Underline(node) => {
                output.push_str("__");
                output.push_str(&serialize_inlines_with_context(
                    &node.children,
                    options,
                    context.opening_span(),
                )?);
                output.push_str("__");
            }
            Inline::Delete(node) => {
                let children = serialize_inlines_with_context(
                    &node.children,
                    options,
                    context.opening_span().delimited_by('~'),
                )?;
                let marker = match node.marker {
                    DeleteMarker::SingleTilde => "~",
                    DeleteMarker::DoubleTilde => "~~",
                };
                output.push_str(marker);
                output.push_str(&children);
                output.push_str(marker);
            }
            Inline::Insert(node) => {
                output.push_str("++");
                output.push_str(&serialize_inlines_with_context(
                    &node.children,
                    options,
                    context.opening_span().delimited_by('+'),
                )?);
                output.push_str("++");
            }
            Inline::Mark(node) => {
                output.push_str("==");
                output.push_str(&serialize_inlines_with_context(
                    &node.children,
                    options,
                    context.opening_span().delimited_by('='),
                )?);
                output.push_str("==");
            }
            Inline::Subscript(node) => {
                output.push('~');
                output.push_str(&serialize_inlines_with_context(
                    &node.children,
                    options,
                    context.opening_span().delimited_by('~'),
                )?);
                output.push('~');
            }
            Inline::Superscript(node) => {
                output.push('^');
                output.push_str(&serialize_inlines_with_context(
                    &node.children,
                    options,
                    context.opening_span().delimited_by('^'),
                )?);
                output.push('^');
            }
            Inline::Spoiler(node) => {
                output.push_str("||");
                output.push_str(&serialize_inlines_with_context(
                    &node.children,
                    options,
                    context.opening_span().delimited_by('|'),
                )?);
                output.push_str("||");
            }
            Inline::Shortcode(node) => {
                output.push(':');
                output.push_str(&node.name);
                output.push(':');
            }
            Inline::Code(node) => {
                if node.fence_length > 0 && !node.raw.is_empty() {
                    let fence = "`".repeat(node.fence_length);
                    let raw = if context.table_cell {
                        table_cell_escape_code_pipes(&node.raw)
                    } else {
                        node.raw.clone()
                    };
                    output.push_str(&fence);
                    output.push_str(&raw);
                    output.push_str(&fence);
                    continue;
                }
                if node.value.is_empty() {
                    output.push_str("`` ``");
                    continue;
                }
                let value = if context.table_cell {
                    table_cell_escape_code_pipes(&node.value)
                } else {
                    node.value.clone()
                };
                let fence = inline_code_fence(&value);
                output.push_str(&fence);
                if code_span_needs_padding(&value) {
                    output.push(' ');
                    output.push_str(&value);
                    output.push(' ');
                } else {
                    output.push_str(&value);
                }
                output.push_str(&fence);
            }
            Inline::Link(node) => {
                escape_trailing_bang(&mut output);
                output.push('[');
                output.push_str(&serialize_inlines_with_context(
                    &node.children,
                    options,
                    context,
                )?);
                output.push_str("](");
                output.push_str(&serialize_destination_kind(
                    &node.destination,
                    node.destination_kind,
                    context,
                ));
                if let (Some(title), Some(title_kind)) = (&node.title, node.title_kind) {
                    output.push(' ');
                    output.push_str(&serialize_title_kind(title, title_kind, context));
                }
                output.push(')');
            }
            Inline::Image(node) => {
                output.push_str("![");
                output.push_str(&serialize_inlines_with_context(
                    &node.alt, options, context,
                )?);
                output.push_str("](");
                output.push_str(&serialize_destination_kind(
                    &node.destination,
                    node.destination_kind,
                    context,
                ));
                if let (Some(title), Some(title_kind)) = (&node.title, node.title_kind) {
                    output.push(' ');
                    output.push_str(&serialize_title_kind(title, title_kind, context));
                }
                output.push(')');
            }
            Inline::LinkReference(node) => {
                let children = serialize_inlines_with_context(&node.children, options, context)?;
                let children_identifier = normalize_reference_label(&children);
                escape_trailing_bang(&mut output);
                push_reference_body(
                    &mut output,
                    node.kind,
                    &children,
                    children_identifier == node.identifier,
                    &reference_explicit_label(node.meta.span.is_some(), &node.label, context),
                );
            }
            Inline::ImageReference(node) => {
                let alt = serialize_inlines_with_context(&node.alt, options, context)?;
                let alt_identifier = normalize_reference_label(&alt);
                output.push('!');
                push_reference_body(
                    &mut output,
                    node.kind,
                    &alt,
                    alt_identifier == node.identifier,
                    &reference_explicit_label(node.meta.span.is_some(), &node.label, context),
                );
            }
            Inline::Autolink(node) => match &node.kind {
                AutolinkKind::Angle => {
                    output.push('<');
                    output.push_str(&node.destination);
                    output.push('>');
                }
                // A GFM literal autolink re-emits its original source text,
                // which re-parses to the same literal (the synthesized
                // `http://`/`mailto:` destination is reconstructed on parse).
                AutolinkKind::GfmLiteral { original } => {
                    // Bare-email literals (`destination` is the original with a
                    // synthesized `mailto:` prefix) re-anchor leftward over
                    // email-local chars on reparse; guard the preceding char.
                    let is_bare_email = node.destination == alloc::format!("mailto:{original}");
                    let follows_literal_email_plus = original.starts_with('+')
                        && index
                            .checked_sub(1)
                            .is_some_and(|prev| is_gfm_literal_email(&inlines[prev]));
                    if is_bare_email && !follows_literal_email_plus {
                        escape_trailing_email_local(&mut output);
                    } else {
                        escape_trailing_less_than(&mut output);
                    }
                    output.push_str(original);
                }
            },
            Inline::Html(node) => output.push_str(&node.value),
            // A break that opens a line, as `&#x20;\n` and `&#x20; \n` parse
            // (the whitespace before a line ending is dropped), is written the
            // same way: a bare line ending or spaces there would end the block.
            // So is one that opens a delimited span, whose opener a line ending
            // right after it would keep from opening.
            Inline::SoftBreak(_) => {
                if breaks_line_start(&output, &mut output_line, opens_line)
                    || (opens_span && output.is_empty())
                {
                    output.push_str("&#x20;");
                }
                output.push('\n');
            }
            Inline::LineBreak(node) => match node.kind {
                LineBreakKind::Backslash => output.push_str("\\\n"),
                LineBreakKind::Spaces
                    if breaks_line_start(&output, &mut output_line, opens_line)
                        || (opens_span && output.is_empty()) =>
                {
                    output.push_str("&#x20; \n");
                }
                LineBreakKind::Spaces => output.push_str("  \n"),
            },
            Inline::Math(node) => {
                output.push_str(&serialize_inline_math_with_context(node, context)?);
            }
            Inline::FootnoteReference(node) => {
                escape_trailing_bang(&mut output);
                output.push_str("[^");
                if node.meta.span.is_some() {
                    output.push_str(&escape_footnote_label_source(&node.label));
                } else {
                    output.push_str(&escape_footnote_label_semantic(&node.label));
                }
                output.push(']');
            }
            Inline::InlineFootnote(node) => {
                output.push_str("^[");
                output.push_str(&serialize_inlines_with_context(
                    &node.children,
                    options,
                    context,
                )?);
                output.push(']');
            }
            Inline::WikiLink(node) => {
                escape_trailing_bang(&mut output);
                output.push_str("[[");
                let target = escape_wikilink_part(&node.target);
                let label = escape_wikilink_part(&node.label);
                if node.target == node.label {
                    output.push_str(&target);
                } else {
                    match node.label_order {
                        WikiLinkLabelOrder::AfterPipe => {
                            output.push_str(&target);
                            output.push('|');
                            output.push_str(&label);
                        }
                        WikiLinkLabelOrder::BeforePipe => {
                            output.push_str(&label);
                            output.push('|');
                            output.push_str(&target);
                        }
                    }
                }
                output.push_str("]]");
            }
            Inline::MdxExpression(node) => {
                output.push('{');
                output.push_str(&node.value);
                output.push('}');
            }
            Inline::MdxJsx(node) => output.push_str(&node.value),
            Inline::TextDirective(node) => {
                output.push(':');
                output.push_str(&node.name);
                output.push_str(&serialize_directive_label_with_context(
                    &node.label,
                    options,
                    context,
                )?);
                output.push_str(&serialize_attributes_with_context(
                    &node.attributes,
                    context,
                ));
            }
        }
        if matches!(
            inline,
            Inline::FootnoteReference(_) | Inline::LinkReference(_) | Inline::ImageReference(_)
        ) && output[segment_start..].contains('`')
        {
            raw_backtick_before = true;
        }
    }
    Ok(output)
}

fn serialize_directive_label(
    label: &[Inline],
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
    serialize_directive_label_with_context(label, options, InlineSerializeContext::default())
}

fn serialize_directive_label_with_context(
    label: &[Inline],
    options: &SerializeOptions,
    context: InlineSerializeContext,
) -> Result<String, SerializeError> {
    if label.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!(
            "[{}]",
            serialize_inlines_with_context(label, options, context)?
        ))
    }
}

fn serialize_attributes(attributes: &[DirectiveAttribute]) -> String {
    serialize_attributes_with_context(attributes, InlineSerializeContext::default())
}

fn serialize_attributes_with_context(
    attributes: &[DirectiveAttribute],
    context: InlineSerializeContext,
) -> String {
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
                output.push('=');
                output.push('"');
                output.push_str(&escape_title_with_context(
                    value,
                    LinkTitleKind::DoubleQuote,
                    context,
                ));
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

/// Whether whitespace ending the text at `index` would be dropped: before a
/// line ending, two-space hard break, or the end of the run. A backslash hard
/// break keeps the whitespace before it.
fn text_is_at_line_end(inlines: &[Inline], index: usize) -> bool {
    matches!(
        inlines.get(index + 1),
        None | Some(Inline::SoftBreak(_))
            | Some(Inline::LineBreak(LineBreak {
                kind: LineBreakKind::Spaces,
                ..
            }))
    )
}

#[cfg(test)]
mod escape_scan_tests;

/// The attention markers whose closer search `TextScan` memoizes.
const ATTENTION_MARKERS: [&str; 5] = ["*", "_", "++", "==", "~~"];

/// Memoized answers to the "could this char start a construct on reparse"
/// questions `escape_text_with_context` asks about each char of one text. Each
/// question looks at the rest of the text, so a fresh scan per char would make
/// escaping one long text quadratic.
struct TextScan<'a> {
    input: &'a str,
    marker_starts: [Positions; ATTENTION_MARKERS.len()],
    marker_closers: [PathMemo; ATTENTION_MARKERS.len()],
    last_occurrences: Vec<(&'static str, Option<usize>)>,
    dollar_runs: Option<SameCharRuns>,
    /// The byte, start, and end of the last run `run_len_from` measured.
    current_run: Option<(u8, usize, usize)>,
    /// The [`DelimiterChars`] that the inlines after the text may write.
    written_later: DelimiterChars,
    /// Those that the inlines before it may write.
    written_before: DelimiterChars,
}

impl<'a> TextScan<'a> {
    #[cfg(test)]
    fn new(input: &'a str, written_later: DelimiterChars) -> Self {
        Self::around(input, written_later, DelimiterChars(0))
    }

    fn around(
        input: &'a str,
        written_later: DelimiterChars,
        written_before: DelimiterChars,
    ) -> Self {
        Self {
            input,
            marker_starts: Default::default(),
            marker_closers: Default::default(),
            last_occurrences: Vec::new(),
            dollar_runs: None,
            current_run: None,
            written_later,
            written_before,
        }
    }

    /// Whether an inline after the text may write `marker`'s char, which a
    /// delimiter opening in the text could pair with.
    fn written_later(&self, marker: &str) -> bool {
        marker
            .chars()
            .next()
            .is_some_and(|char| self.written_later.contains(char))
    }

    /// `same_char_run_len` for an ASCII `needle`, measuring each run once
    /// however many of its positions ask.
    fn run_len_from(&mut self, needle: u8, offset: usize) -> usize {
        if let Some((byte, start, end)) = self.current_run {
            if byte == needle && start <= offset && offset <= end {
                return end - offset;
            }
        }
        let end = offset
            + self.input.as_bytes()[offset..]
                .iter()
                .take_while(|byte| **byte == needle)
                .count();
        self.current_run = Some((needle, offset, end));
        end - offset
    }

    /// Whether `pattern` occurs starting at or after `from`.
    fn occurs_from(&mut self, pattern: &'static str, from: usize) -> bool {
        let last = match self
            .last_occurrences
            .iter()
            .find(|(cached, _)| *cached == pattern)
        {
            Some((_, last)) => *last,
            None => {
                let last = self.input.rfind(pattern);
                self.last_occurrences.push((pattern, last));
                last
            }
        };
        last.is_some_and(|last| last >= from)
    }

    /// Whether a run of `marker` that can close starts at or after `from`,
    /// stepping from one occurrence to the position after it.
    fn attention_closer_follows(&mut self, marker: &str, from: usize, underscore: bool) -> bool {
        let input = self.input;
        let slot = ATTENTION_MARKERS
            .iter()
            .position(|known| *known == marker)
            .expect("attention marker");
        let marker = ATTENTION_MARKERS[slot];
        let starts = &mut self.marker_starts[slot];
        self.marker_closers[slot]
            .resolve(input.len() + 1, from, |cursor| {
                let Some(candidate) =
                    starts.first_at_or_after(cursor, || pattern_starts(input, marker))
                else {
                    return Step::Done(None);
                };
                if !input[candidate + marker.len()..].starts_with(marker)
                    && text_delimiter_can_close(input, candidate, marker.len(), underscore)
                {
                    Step::Done(Some(candidate))
                } else {
                    Step::Next(candidate + marker.len())
                }
            })
            .is_some()
    }

    /// Whether some position at or after `from` begins exactly `run_len`
    /// trailing bytes of a run of `$`.
    fn exact_dollar_run_follows(&mut self, from: usize, run_len: usize) -> bool {
        let input = self.input;
        let runs = self
            .dollar_runs
            .get_or_insert_with(|| SameCharRuns::new(input, b'$'));
        runs.has_run_ending_at_or_after(from + run_len, run_len)
    }
}

/// The maximal runs of one ASCII byte, ordered by end, with the longest run
/// among each suffix.
struct SameCharRuns {
    ends: Vec<usize>,
    longest_from: Vec<usize>,
}

impl SameCharRuns {
    fn new(input: &str, needle: u8) -> Self {
        let bytes = input.as_bytes();
        let mut ends = Vec::new();
        let mut lens = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == needle {
                let start = index;
                while bytes.get(index) == Some(&needle) {
                    index += 1;
                }
                ends.push(index);
                lens.push(index - start);
            } else {
                index += 1;
            }
        }
        let mut longest_from = lens;
        for index in (0..longest_from.len().saturating_sub(1)).rev() {
            longest_from[index] = longest_from[index].max(longest_from[index + 1]);
        }
        Self { ends, longest_from }
    }

    /// Whether a run at least `run_len` long ends at or after `min_end`.
    fn has_run_ending_at_or_after(&self, min_end: usize, run_len: usize) -> bool {
        let first = self.ends.partition_point(|end| *end < min_end);
        self.longest_from
            .get(first)
            .is_some_and(|longest| *longest >= run_len)
    }
}

fn escape_text_with_context(
    input: &str,
    preserve_leading: bool,
    preserve_trailing: bool,
    context: InlineSerializeContext,
) -> String {
    let avoid_star_edges = context.avoid_star_edges;
    let in_underscore_emphasis = context.in_underscore_emphasis;
    let mut output = String::with_capacity(input.len() + input.len() / 8);
    let mut output_line = OutputLine::default();
    let mut line_digit_prefix = 0usize;
    // Only the space or tab at a preserved edge is written as a reference:
    // the ones beside it are no longer at the edge of the line or span.
    let trailing_start = if preserve_trailing && input.ends_with([' ', '\t']) {
        input.len() - 1
    } else {
        input.len()
    };
    // Delimiter decisions read the text as the reparse sees it, where each
    // char written as a character reference is punctuation.
    let view = referenced_chars_as_punctuation(input, preserve_leading, trailing_start);
    let view = view.as_ref();
    let mut scan = TextScan::around(view, context.written_later, context.written_before);
    // The end of the current `$`, `*`, and `_` run, and whether it is escaped.
    let mut dollar_run = (0usize, false);
    let mut star_run = (0usize, false);
    let mut underscore_run = (0usize, false);
    let mut tilde_run = (0usize, false);
    let mut pipe_run = (0usize, false);
    let mut chars = input.char_indices().peekable();
    let mut at_leading_edge = preserve_leading;
    while let Some((offset, char)) = chars.next() {
        if char == '\n' {
            output.push_str("&#xA;");
            at_leading_edge = false;
            continue;
        }
        if char == '\r' {
            output.push_str("&#xD;");
            at_leading_edge = false;
            continue;
        }
        if (at_leading_edge || offset >= trailing_start) && char == ' ' {
            output.push_str("&#x20;");
            at_leading_edge = false;
            continue;
        }
        if (at_leading_edge || offset >= trailing_start) && char == '\t' {
            output.push_str("&#x9;");
            at_leading_edge = false;
            continue;
        }
        // A tab inside the text stays literal: the reparse keeps it as it is.
        if written_as_reference(char) {
            output.push_str(&format!("&#x{:X};", char as u32));
            at_leading_edge = false;
            continue;
        }
        at_leading_edge = false;
        if line_digit_prefix == output_line.len(&output) && char.is_ascii_digit() {
            output.push(char);
            line_digit_prefix += 1;
            continue;
        }
        // `://` can open a literal autolink: with any scheme, or none under
        // the relaxed autolink dialect.
        if char == ':' && input[offset + char.len_utf8()..].starts_with("//") {
            output.push('\\');
            output.push(char);
            line_digit_prefix = usize::MAX;
            continue;
        }
        if char == '.' && input[..offset].ends_with("www") {
            output.push('\\');
            output.push(char);
            line_digit_prefix = usize::MAX;
            continue;
        }
        if char == '@' {
            if at_sign_can_start_email_autolink(input, offset) {
                output.push_str("&#x40;");
            } else {
                output.push(char);
            }
            line_digit_prefix = usize::MAX;
            continue;
        }
        if line_digit_prefix != usize::MAX
            && line_digit_prefix > 0
            && matches!(char, '.' | ')')
            && chars
                .peek()
                .map(|(_, next)| next.is_whitespace())
                .unwrap_or(true)
        {
            output.push('\\');
            output.push(char);
            line_digit_prefix = usize::MAX;
            continue;
        }
        if output_line.len(&output) == 0
            && matches!(char, '-' | '+')
            && chars
                .peek()
                .map(|(_, next)| next.is_whitespace())
                .unwrap_or(true)
        {
            output.push('\\');
            output.push(char);
            line_digit_prefix = usize::MAX;
            continue;
        }
        if output_line.len(&output) == 0
            && ((char == '-' && chars.peek().is_some_and(|(_, next)| *next == '-')) || char == '=')
        {
            output.push('\\');
            output.push(char);
            line_digit_prefix = usize::MAX;
            continue;
        }
        line_digit_prefix = usize::MAX;
        match char {
            '*' if avoid_star_edges => output.push_str("&#x2A;"),
            '|' if context.table_cell => output.push_str("&#x7C;"),
            '|' if output_line.len(&output) == 0 => {
                output.push('\\');
                output.push(char);
            }
            // A backslash keeps a backtick from opening a code span but not
            // from closing one, so every backtick is escaped: a bare one could
            // open a span that an escaped one closes.
            '`' => {
                output.push('\\');
                output.push(char);
            }
            // Escaping only part of a run would leave a shorter run, which
            // flanks and pairs differently, so a run is escaped whole or not.
            '*' if run_escaped(view, offset, b'*', &mut scan, &mut star_run, |scan, at| {
                text_attention_delimiter_can_start(view, at, "*", false, scan)
            }) =>
            {
                output.push('\\');
                output.push(char);
            }
            '_' if run_escaped(
                view,
                offset,
                b'_',
                &mut scan,
                &mut underscore_run,
                |scan, at| {
                    (in_underscore_emphasis && text_delimiter_can_close(view, at, 1, true))
                        || text_attention_delimiter_can_start(view, at, "_", true, scan)
                },
            ) =>
            {
                output.push('\\');
                output.push(char);
            }
            // A text that starts a line may sit on a paragraph's continuation
            // line, where an HTML block start (types 1–6) or a directive
            // opener would interrupt the paragraph.
            '<' if output_line.len(&output) == 0
                && line_starts_interrupting_html_block(&view[offset..]) =>
            {
                output.push('\\');
                output.push(char);
            }
            // A `:` or `~` and whitespace opening a line would open the
            // details of a description list whose term is the line before.
            ':' | '~'
                if offset == 0
                    && context.text_opens_line
                    && input[offset + 1..]
                        .chars()
                        .next()
                        .is_none_or(|next| matches!(next, ' ' | '\t')) =>
            {
                output.push('\\');
                output.push(char);
            }
            ':' if (output_line.len(&output) == 0 && input[offset..].starts_with("::"))
                || text_directive_can_start(view, offset)
                || shortcode_can_form(view, offset, &mut scan) =>
            {
                output.push('\\');
                output.push(char);
            }
            '<' if text_less_than_can_start_inline(view, offset, &mut scan) => {
                output.push('\\');
                output.push(char);
            }
            '>' if output_line.len(&output) == 0 => {
                output.push('\\');
                output.push(char);
            }
            '{' if scan.occurs_from("}", offset + char.len_utf8()) => {
                output.push('\\');
                output.push(char);
            }
            '#' if text_atx_heading_can_start(view, offset, output_line.len(&output)) => {
                output.push('\\');
                output.push(char);
            }
            // A run of two or more bars opens with its last two, so every bar
            // but the last of a run that can open is written as a reference.
            '|' if run_escaped(view, offset, b'|', &mut scan, &mut pipe_run, |scan, at| {
                text_spoiler_can_start(view, at, scan)
            }) && offset + 1 < pipe_run.0 =>
            {
                output.push_str("&#x7C;")
            }
            // Escaping only part of a `$` run would leave a shorter run that can
            // open math, so a run is escaped whole or not at all.
            '$' if run_escaped(
                view,
                offset,
                b'$',
                &mut scan,
                &mut dollar_run,
                |scan, at| text_math_can_start(view, at, scan),
            ) =>
            {
                output.push('\\');
                output.push(char);
            }
            '!' if input[offset + char.len_utf8()..].starts_with('[') => {
                output.push('\\');
                output.push(char);
            }
            // A run of three opening a line would open a code fence.
            '~' if run_escaped(view, offset, b'~', &mut scan, &mut tilde_run, |scan, at| {
                (at == 0 && context.text_opens_line && same_byte_run_len(view, at, b'~') >= 3)
                    || tilde_run_can_pair(view, at, scan)
            }) =>
            {
                output.push('\\');
                output.push(char);
            }
            '^' if text_caret_can_start(view, offset, &mut scan) => {
                output.push('\\');
                output.push(char);
            }
            '+' if text_attention_delimiter_can_start(view, offset, "++", false, &mut scan) => {
                output.push('\\');
                output.push(char);
            }
            '=' if text_attention_delimiter_can_start(view, offset, "==", false, &mut scan) => {
                output.push('\\');
                output.push(char);
            }
            '&' if text_character_reference_can_start(view, offset) => {
                output.push('\\');
                output.push(char);
            }
            '\\' | '[' | ']' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn text_attention_delimiter_can_start(
    input: &str,
    offset: usize,
    marker: &str,
    underscore: bool,
    scan: &mut TextScan,
) -> bool {
    if !input[offset..].starts_with(marker) {
        return false;
    }
    if input[offset + marker.len()..].starts_with(marker)
        || text_char_at_edge(input, offset, marker.len())
    {
        return true;
    }
    if !text_delimiter_can_open(input, offset, marker.len(), underscore) {
        return false;
    }

    scan.written_later(marker)
        || scan.attention_closer_follows(marker, offset + marker.len(), underscore)
}

fn text_delimiter_can_open(
    input: &str,
    offset: usize,
    marker_len: usize,
    underscore: bool,
) -> bool {
    let flanking = text_delimiter_flanking(input, offset, marker_len);
    if touches_tilde_bonus(input, offset, flanking.next) {
        return true;
    }
    if underscore {
        flanking.left && (!flanking.right || flanking.previous.is_some_and(is_flanking_punctuation))
    } else {
        flanking.left
    }
}

/// Whether the `*` or `_` run at `offset` touches a `~` on the side whose char
/// is `neighbour`: the GFM strikethrough bonus lets such a run open or close
/// whatever its flanking.
fn touches_tilde_bonus(input: &str, offset: usize, neighbour: Option<char>) -> bool {
    neighbour == Some('~') && matches!(input.as_bytes()[offset], b'*' | b'_')
}

fn text_delimiter_can_close(
    input: &str,
    offset: usize,
    marker_len: usize,
    underscore: bool,
) -> bool {
    let flanking = text_delimiter_flanking(input, offset, marker_len);
    if touches_tilde_bonus(input, offset, flanking.previous) {
        return true;
    }
    if underscore {
        flanking.right && (!flanking.left || flanking.next.is_some_and(is_flanking_punctuation))
    } else {
        flanking.right
    }
}

#[derive(Clone, Copy)]
struct TextDelimiterFlanking {
    left: bool,
    right: bool,
    previous: Option<char>,
    next: Option<char>,
}

fn text_delimiter_flanking(input: &str, offset: usize, marker_len: usize) -> TextDelimiterFlanking {
    let previous = input[..offset].chars().next_back();
    let next = input[offset + marker_len..].chars().next();

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

    TextDelimiterFlanking {
        left,
        right,
        previous,
        next,
    }
}

fn text_less_than_can_start_inline(input: &str, offset: usize, scan: &mut TextScan) -> bool {
    let after_offset = offset + '<'.len_utf8();
    let after = &input[after_offset..];
    if scan.written_later(">") || scan.occurs_from(">", after_offset) {
        let next = after.chars().next();
        return next.is_some_and(|char| {
            char.is_ascii_alphabetic() || matches!(char, '/' | '!' | '?' | '_')
        }) || after.starts_with("http://")
            || after.starts_with("https://")
            || scan.occurs_from("@", after_offset);
    }
    false
}

/// Whether the `:` at `offset` can open a shortcode, closed by a `:` after a
/// name in the text or written by the inlines after it.
fn shortcode_can_form(input: &str, offset: usize, scan: &mut TextScan) -> bool {
    let name_end = offset
        + 1
        + input[offset + 1..]
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+'))
            .count();
    let opens = name_end > offset + 1
        && match input.as_bytes().get(name_end) {
            Some(b':') => true,
            Some(_) => false,
            None => scan.written_later(":"),
        };
    opens
}

/// Whether the `:` at `offset` can open a text directive: after whitespace,
/// `(`, `[`, `{`, or at the text's start, and before a directive name.
fn text_directive_can_start(input: &str, offset: usize) -> bool {
    let rest = &input[offset + ':'.len_utf8()..];
    let opens_after = input[..offset]
        .chars()
        .next_back()
        .is_none_or(|char| char.is_whitespace() || matches!(char, '(' | '[' | '{'));
    let name_len = rest
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        .count();
    opens_after && is_directive_name(&rest[..name_len])
}

fn text_atx_heading_can_start(input: &str, offset: usize, output_line_len: usize) -> bool {
    if output_line_len != 0 {
        return false;
    }
    let hashes = same_char_run_len(input, offset, '#');
    (1..=6).contains(&hashes)
        && input[offset + hashes..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

fn text_spoiler_can_start(input: &str, offset: usize, scan: &mut TextScan) -> bool {
    input[offset..].starts_with("||")
        && !input[offset + "||".len()..].starts_with('|')
        && (scan.written_later("|") || scan.occurs_from("||", offset + "||".len()))
}

/// Whether the `byte` at `offset` is escaped: its whole run is when
/// `escapes` holds at any position in it. `run` holds the end of the run read
/// last and its answer, which the run's later bytes reuse.
fn run_escaped(
    input: &str,
    offset: usize,
    byte: u8,
    scan: &mut TextScan,
    run: &mut (usize, bool),
    mut escapes: impl FnMut(&mut TextScan, usize) -> bool,
) -> bool {
    if offset >= run.0 {
        let end = offset + same_byte_run_len(input, offset, byte);
        let escaped = (offset..end).any(|position| escapes(scan, position));
        *run = (end, escaped);
    }
    run.1
}

fn same_byte_run_len(input: &str, offset: usize, byte: u8) -> usize {
    input.as_bytes()[offset..]
        .iter()
        .take_while(|item| **item == byte)
        .count()
}

/// `input` with every char that the text escaper writes as a character
/// reference replaced by as many `&` bytes: control chars other than a tab,
/// and the spaces and tabs it keeps at a preserved edge.
/// Whether text writes `char` as a character reference: a control char
/// other than the tab, line tabulation, form feed, and next line, which the
/// reparse keeps as written and reads as whitespace, as the source did.
fn written_as_reference(char: char) -> bool {
    char.is_control() && !matches!(char, '\t' | '\u{b}' | '\u{c}' | '\u{85}')
}

fn referenced_chars_as_punctuation(
    input: &str,
    preserve_leading: bool,
    trailing_start: usize,
) -> Cow<'_, str> {
    let leading_end = usize::from(preserve_leading && input.starts_with([' ', '\t']));
    let referenced = |offset: usize, char: char| {
        written_as_reference(char)
            || (matches!(char, ' ' | '\t') && (offset < leading_end || offset >= trailing_start))
    };
    // A control char is ASCII below a space or DEL, or a C1 char, whose UTF-8
    // form opens with 0xC2; so the bytes tell when no char is referenced.
    let bytes = input.as_bytes();
    let may_reference = leading_end > 0
        || trailing_start < input.len()
        || bytes
            .iter()
            .any(|byte| (*byte < b' ' && *byte != b'\t') || matches!(*byte, 0x7f | 0xc2));
    if !may_reference
        || !input
            .char_indices()
            .any(|(offset, char)| referenced(offset, char))
    {
        return Cow::Borrowed(input);
    }
    let mut view = String::with_capacity(input.len());
    for (offset, char) in input.char_indices() {
        if referenced(offset, char) {
            view.extend(core::iter::repeat_n('&', char.len_utf8()));
        } else {
            view.push(char);
        }
    }
    Cow::Owned(view)
}

fn text_math_can_start(input: &str, offset: usize, scan: &mut TextScan) -> bool {
    // Mirror the parser's dollar-math start (code-span analogue): an opening run
    // of N dollars starts math when an exact-length-N closing run exists ahead.
    // Edge whitespace no longer blocks it, so a literal `$` adjacent to such a
    // run must be escaped to avoid forming math on the round trip.
    let marker_len = scan.run_len_from(b'$', offset);
    if marker_len == 0 || text_char_at_edge(input, offset, marker_len) {
        return true;
    }
    scan.written_later("$") || scan.exact_dollar_run_follows(offset + marker_len, marker_len)
}

/// Whether the `~` run from `offset` to its end could pair, or join a run,
/// once written literally: with a `~` later in the text or one the inlines
/// after it (or the span around it) write, or with one written right before
/// the text's start. (A run before it in the text is escaped when it could
/// pair with this one.) Escaping only when it could keeps a `*` or `_` run
/// beside a literal `~` opening or closing as it did.
fn tilde_run_can_pair(input: &str, offset: usize, scan: &mut TextScan) -> bool {
    let end = offset + same_byte_run_len(input, offset, b'~');
    scan.occurs_from("~", end)
        || scan.written_later("~")
        || (offset == 0 && scan.written_before.contains('~'))
}

fn text_caret_can_start(input: &str, offset: usize, scan: &mut TextScan) -> bool {
    input[offset + '^'.len_utf8()..].starts_with('[')
        || text_simple_delimiter_can_start(input, offset, "^", scan)
}

fn text_simple_delimiter_can_start(
    input: &str,
    offset: usize,
    marker: &'static str,
    scan: &mut TextScan,
) -> bool {
    if text_char_at_edge(input, offset, marker.len())
        || input[offset + marker.len()..].starts_with(marker)
        || input[..offset].ends_with(marker)
    {
        return true;
    }
    scan.written_later(marker) || scan.occurs_from(marker, offset + marker.len())
}

fn text_character_reference_can_start(input: &str, offset: usize) -> bool {
    let after = &input[offset + '&'.len_utf8()..];
    if let Some(rest) = after.strip_prefix('#') {
        let (digits, rest) = if let Some(hex) = rest.strip_prefix(['x', 'X']) {
            (
                hex.chars()
                    .take_while(|char| char.is_ascii_hexdigit())
                    .count(),
                hex,
            )
        } else {
            (
                rest.chars()
                    .take_while(|char| char.is_ascii_digit())
                    .count(),
                rest,
            )
        };
        return digits > 0 && rest[digits..].starts_with(';');
    }

    let name_len = after
        .chars()
        .take_while(|char| char.is_ascii_alphanumeric())
        .count();
    name_len > 0 && after[name_len..].starts_with(';')
}

fn text_char_at_edge(input: &str, offset: usize, len: usize) -> bool {
    offset == 0 || offset + len >= input.len()
}

fn same_char_run_len(input: &str, offset: usize, needle: char) -> usize {
    input[offset..]
        .chars()
        .take_while(|char| *char == needle)
        .map(char::len_utf8)
        .sum()
}

fn at_sign_can_start_email_autolink(input: &str, offset: usize) -> bool {
    let before = input[..offset]
        .chars()
        .next_back()
        .is_some_and(|char| char.is_ascii_alphanumeric());
    if !before {
        return false;
    }

    let mut saw_domain_char = false;
    let mut saw_dot = false;
    let mut saw_domain_char_after_dot = false;
    for char in input[offset + 1..].chars() {
        if char.is_ascii_alphanumeric() {
            saw_domain_char = true;
            if saw_dot {
                saw_domain_char_after_dot = true;
            }
            continue;
        }
        if char == '.' && saw_domain_char {
            saw_dot = true;
            continue;
        }
        if matches!(char, '-' | '_') && saw_domain_char {
            continue;
        }
        break;
    }
    saw_domain_char_after_dot
}

/// Whether `output` ends with `char` that no backslash escapes.
fn ends_with_unescaped(output: &str, char: char) -> bool {
    output.strip_suffix(char).is_some_and(|before| {
        let backslashes = before.len() - before.trim_end_matches('\\').len();
        backslashes % 2 == 0
    })
}

/// Whether the next inline opens a line of the block: at the start of
/// inlines that open one, or after a line ending.
fn breaks_line_start(output: &str, output_line: &mut OutputLine, opens_line: bool) -> bool {
    output_line.len(output) == 0 && (opens_line || !output.is_empty())
}

/// The length of the last line of an append-only output, found by scanning only
/// the bytes appended since the previous call, so asking once per appended
/// char stays linear on a long line.
#[derive(Default)]
struct OutputLine {
    line_start: usize,
    scanned: usize,
}

impl OutputLine {
    fn len(&mut self, output: &str) -> usize {
        if let Some(newline) = output.as_bytes()[self.scanned..]
            .iter()
            .rposition(|byte| *byte == b'\n')
        {
            self.line_start = self.scanned + newline + 1;
        }
        self.scanned = output.len();
        output.len() - self.line_start
    }
}

fn escape_destination_with_pipe(input: &str, escape_pipe: bool) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            // A space would end the destination, and `\ ` is no escape.
            char if char.is_control() || char == ' ' => {
                output.push_str(&format!("&#x{:X};", char as u32));
            }
            '|' if escape_pipe => {
                output.push('\\');
                output.push(char);
            }
            '(' | ')' | '\\' | '<' | '>' | '&' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

/// Normalize a serialized reference label exactly the way the parser matches
/// reference labels: collapse internal whitespace and Unicode case-fold the RAW
/// text (no backslash/entity unescape). Delegating to the parser's
/// `normalize_label` keeps this in lockstep so the Shortcut/Collapsed arms
/// decide correctly whether the rendered children already reproduce the
/// definition identifier.
fn normalize_reference_label(input: &str) -> String {
    crate::parse::normalize_label(input)
}

/// Emit the bracketed body of a link/image reference (`[text]`, `[text][]`, or
/// `[text][label]`) given the already-serialized `rendered` children and the
/// escaped raw `label`.
///
/// A Shortcut/Collapsed reference normally re-uses the rendered children as the
/// matching label, so it is only whole if those children fold back to the
/// definition identifier. Under RAW label matching the children can re-escape
/// in a fold-breaking way (e.g. a leading `^` becomes `\^`), so when the
/// children no longer reproduce the identifier we substitute the escaped raw
/// label as the bracket body — keeping the Shortcut/Collapsed kind (and its
/// re-parse) intact instead of degrading it into a Full reference.
fn push_reference_body(
    output: &mut String,
    kind: ReferenceKind,
    rendered: &str,
    children_match_identifier: bool,
    escaped_label: &str,
) {
    // For a Shortcut/Collapsed reference the bracket body must fold back to the
    // identifier on its own. Substitute the escaped raw label when the rendered
    // children would not (keeping the reference kind), but a Full reference
    // always keeps its rendered text since its explicit label does the matching.
    let use_label_body = !children_match_identifier && !matches!(kind, ReferenceKind::Full);
    let body = if use_label_body {
        escaped_label
    } else {
        rendered
    };

    output.push('[');
    output.push_str(body);
    output.push(']');

    match kind {
        ReferenceKind::Shortcut => {}
        ReferenceKind::Collapsed => output.push_str("[]"),
        ReferenceKind::Full => {
            output.push('[');
            output.push_str(escaped_label);
            output.push(']');
        }
    }
}

/// Escape the explicit label of a link/image reference. The original `label`
/// (not the normalized identifier) is used so case and entity spelling survive
/// the round-trip. A parsed label (`span.is_some()`) is already source text, so
/// only control characters are escaped; a hand-built label is semantic text and
/// is escaped like any reference label.
fn reference_explicit_label(
    from_source: bool,
    label: &str,
    context: InlineSerializeContext,
) -> String {
    if from_source {
        escape_reference_label_source(label, context.table_cell)
    } else {
        escape_reference_label_with_pipe(label, context.table_cell)
    }
}

/// Escapes a parsed definition label for re-emission. A definition label may
/// span several physical lines (CommonMark §4.7), and the parser stores those
/// interior newlines verbatim in `label`. Emitting them as literal line breaks
/// (rather than `&#xA;`) lets the multi-line label re-parse to the same raw
/// label, keeping the round trip stable; other control characters are still
/// numeric-escaped, and tabs pass through as in `escape_reference_label_source`.
fn escape_definition_label_source(input: &str) -> String {
    escape_reference_label_source(input, false)
}

fn escape_reference_label_source(input: &str, escape_pipe: bool) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            // A reference label may span several physical lines, and the parser
            // matches the RAW label (whitespace collapsed, no entity decode), so
            // every control char, line endings and tabs among them, is written
            // as itself rather than as a reference such as `&#xA;`. This keeps
            // a whitespace-bearing label re-parsing as the same reference —
            // crucially, a `^`-prefixed label with a literal space stays a link
            // reference instead of becoming a footnote.
            char if char.is_control() => output.push(char),
            '|' if escape_pipe => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn escape_reference_label_with_pipe(input: &str, escape_pipe: bool) -> String {
    escape_label_syntax(input, escape_pipe, false)
}

/// A footnote label is matched as written, so its chars are.
fn escape_footnote_label_source(input: &str) -> String {
    input.into()
}

fn escape_footnote_label_semantic(input: &str) -> String {
    escape_label_syntax(input, false, true)
}

fn escape_label_syntax(input: &str, escape_pipe: bool, escape_whitespace: bool) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            char if char.is_whitespace() && escape_whitespace => {
                output.push_str(&format!("&#x{:X};", char as u32));
            }
            char if char.is_control() => output.push_str(&format!("&#x{:X};", char as u32)),
            '|' if escape_pipe => {
                output.push('\\');
                output.push(char);
            }
            '\\' | '[' | ']' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn escape_wikilink_part(input: &str) -> String {
    let mut output = String::new();
    for (offset, char) in input.char_indices() {
        match char {
            char if char.is_control() => output.push_str(&format!("&#x{:X};", char as u32)),
            '&' if text_character_reference_can_start(input, offset) => {
                output.push('\\');
                output.push(char);
            }
            '\\' | '[' | ']' | '|' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn serialize_destination_kind(
    input: &str,
    kind: LinkDestinationKind,
    context: InlineSerializeContext,
) -> String {
    match kind {
        LinkDestinationKind::Omitted if input.is_empty() => String::new(),
        LinkDestinationKind::Angle => {
            let mut output = String::from("<");
            output.push_str(&escape_angle_destination_with_context(input, context));
            output.push('>');
            output
        }
        LinkDestinationKind::Bare | LinkDestinationKind::Omitted => {
            if input.is_empty() {
                "<>".into()
            } else {
                escape_destination_with_pipe(input, context.table_cell)
            }
        }
    }
}

fn escape_angle_destination_with_context(input: &str, context: InlineSerializeContext) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            char if char.is_control() => output.push_str(&format!("&#x{:X};", char as u32)),
            '|' if context.table_cell => {
                output.push('\\');
                output.push(char);
            }
            '\\' | '<' | '>' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn serialize_title_kind(
    input: &str,
    kind: LinkTitleKind,
    context: InlineSerializeContext,
) -> String {
    let (open, close) = match kind {
        LinkTitleKind::DoubleQuote => ('"', '"'),
        LinkTitleKind::SingleQuote => ('\'', '\''),
        LinkTitleKind::Paren => ('(', ')'),
    };
    let mut output = String::new();
    output.push(open);
    output.push_str(&escape_title_with_context(input, kind, context));
    output.push(close);
    output
}

fn escape_title_with_context(
    input: &str,
    kind: LinkTitleKind,
    context: InlineSerializeContext,
) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            char if char.is_control() => output.push_str(&format!("&#x{:X};", char as u32)),
            '|' if context.table_cell => {
                output.push('\\');
                output.push(char);
            }
            '\\' | '&' => {
                output.push('\\');
                output.push(char);
            }
            '"' if kind == LinkTitleKind::DoubleQuote => {
                output.push('\\');
                output.push(char);
            }
            '\'' if kind == LinkTitleKind::SingleQuote => {
                output.push('\\');
                output.push(char);
            }
            '(' | ')' if kind == LinkTitleKind::Paren => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn unordered_list_marker(delimiter: ListDelimiter) -> char {
    match delimiter {
        ListDelimiter::Dash => '-',
        ListDelimiter::Asterisk => '*',
        ListDelimiter::Plus => '+',
        ListDelimiter::Period | ListDelimiter::Paren => '-',
    }
}

fn ordered_list_marker(delimiter: ListDelimiter) -> char {
    match delimiter {
        ListDelimiter::Paren => ')',
        ListDelimiter::Dash
        | ListDelimiter::Asterisk
        | ListDelimiter::Plus
        | ListDelimiter::Period => '.',
    }
}

fn prefix_lines(input: &str, prefix: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    let bytes = input.as_bytes();
    let mut output = String::new();
    let mut line_start = 0;
    let mut cursor = 0;
    while cursor < input.len() {
        let eol_end = match bytes[cursor] {
            b'\n' => Some(cursor + 1),
            b'\r' if bytes.get(cursor + 1) == Some(&b'\n') => Some(cursor + 2),
            b'\r' => Some(cursor + 1),
            _ => None,
        };
        if let Some(end) = eol_end {
            output.push_str(prefix);
            output.push_str(&input[line_start..end]);
            cursor = end;
            line_start = cursor;
        } else {
            cursor += 1;
        }
    }
    if line_start < input.len() {
        output.push_str(prefix);
        output.push_str(&input[line_start..]);
    }
    output
}

fn indent_after_first_line(input: &str, width: usize) -> String {
    let indent = " ".repeat(width);
    input
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                line.into()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn indent_lines(input: &str, width: usize) -> String {
    let indent = " ".repeat(width);
    input
        .lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn indent_continuation(input: &str) -> String {
    input
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                line.into()
            } else {
                format!("    {line}")
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn trim_trailing_newline(input: &str) -> &str {
    input.trim_end_matches('\n').trim_end_matches('\r')
}

fn ends_with_line_ending(input: &str) -> bool {
    input.ends_with('\n') || input.ends_with('\r')
}

fn fence_for(input: &str, marker: FenceMarker, min_len: usize) -> String {
    let char = match marker {
        FenceMarker::Backtick => '`',
        FenceMarker::Tilde => '~',
    };
    let longest = longest_char_streak(input, char);
    char.to_string().repeat(min_len.max(longest + 1))
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
        None => (char.to_string().repeat(min_len), 0),
        Some(indent) if indent > 0 => (char.to_string().repeat(min_len), 4 - indent),
        Some(_) => {
            let mut length = min_len;
            for line in lines() {
                if let Some((_, run)) = closing_like(line, length) {
                    length = run + 1;
                }
            }
            (char.to_string().repeat(length), 0)
        }
    }
}

fn inline_code_fence(input: &str) -> String {
    fence_for(input, FenceMarker::Backtick, 1)
}

fn code_span_needs_padding(input: &str) -> bool {
    input.starts_with('`')
        || input.ends_with('`')
        || (input.starts_with(' ') && input.ends_with(' ') && input.chars().any(|char| char != ' '))
}

fn table_cell_escape_code_pipes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for char in input.chars() {
        if char == '|' {
            output.push('\\');
        }
        output.push(char);
    }
    output
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

fn serialize_inline_math_with_context(
    node: &MathInline,
    context: InlineSerializeContext,
) -> Result<String, SerializeError> {
    // A pipe in a table cell is escaped with the cell (see
    // `escape_cell_delimiter_pipes`), which the table drops again.
    let _ = context;
    let input = node.value.as_str();
    match node.kind {
        MathInlineKind::Code => {
            if input.contains("`$") {
                return Err(SerializeError::UnsupportedNode(
                    "inline math (code-math form) containing a `$` close",
                ));
            }
            Ok(format!("$`{input}`$"))
        }
        // Dollar math is emitted verbatim behind an exact-length fence: no
        // padding strip and no fence widening. A single-`$` value can only
        // contain a `$` that is backslash-escaped (`\$`), which the flanking
        // parser skips, so an exact `$`…`$` fence round-trips; a `$$` display
        // value is verbatim including any edge spaces or newlines.
        MathInlineKind::Dollar { dollars } => {
            let fence = "$".repeat(usize::from(dollars));
            Ok(format!("{fence}{input}{fence}"))
        }
    }
}

/// Whether a serialized cell would split into more cells when parsed back,
/// judged with spoilers enabled so a spoiler's bars are never taken for
/// delimiters.
/// `cell` with a backslash before each pipe that would delimit a cell: a pipe
/// that raw HTML, an autolink, or another verbatim inline writes. The table
/// drops that backslash before the cell's inline parse.
fn escape_cell_delimiter_pipes(cell: String) -> String {
    if !cell.contains('|') {
        return cell;
    }
    // A pipe after a backslash never delimits, and the cell's own text writes
    // its pipes as references, so most cells hold no other.
    let bytes = cell.as_bytes();
    if !(0..bytes.len())
        .any(|index| bytes[index] == b'|' && (index == 0 || bytes[index - 1] != b'\\'))
    {
        return cell;
    }
    let delimiters = crate::parse::table_row_delimiters(&cell, true);
    if delimiters.is_empty() {
        return cell;
    }
    let mut output = String::with_capacity(cell.len() + delimiters.len());
    let mut copied = 0;
    for pipe in delimiters {
        output.push_str(&cell[copied..pipe]);
        output.push('\\');
        copied = pipe;
    }
    output.push_str(&cell[copied..]);
    output
}

fn longest_char_streak(input: &str, needle: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for char in input.chars() {
        if char == needle {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn directive_fence(inner: &str) -> String {
    ":".repeat(directive_fence_len(inner))
}

fn directive_fence_len(inner: &str) -> usize {
    let mut max = 3;
    for line in inner.lines() {
        if let Some(length) = directive_closing_fence_len(line) {
            max = max.max(length + 1);
        }
    }
    max
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
